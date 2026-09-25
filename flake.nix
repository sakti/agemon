{
  description = "Agent monitoring - push system metrics to Prometheus remote write";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    crane,
  }: let
    supportedSystems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
    forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

    mkPackage = system: let
      overlays = [(import rust-overlay)];
      pkgs = import nixpkgs {
        inherit system overlays;
      };

      rustToolchain = pkgs.rust-bin.stable.latest.minimal;
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
      src = craneLib.cleanCargoSource ./.;

      commonArgs = {
        inherit src;
        inherit (craneLib.crateNameFromCargoToml {inherit src;}) pname version;

        buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.libiconv
          pkgs.apple-sdk_15
        ];
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in
      craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
          doCheck = false;
        });

    mkDockerImage = system: let
      overlays = [(import rust-overlay)];
      pkgs = import nixpkgs {inherit system overlays;};
      muslPkgs = pkgs.pkgsMusl;

      rustTargetTriple =
        if system == "x86_64-linux"
        then "x86_64-unknown-linux-musl"
        else "aarch64-unknown-linux-musl";

      rustToolchain = pkgs.rust-bin.stable.latest.minimal.override {
        targets = [rustTargetTriple];
      };
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
      src = craneLib.cleanCargoSource ./.;

      commonArgs =
        {
          inherit src;
          inherit (craneLib.crateNameFromCargoToml {inherit src;}) pname version;
          CARGO_BUILD_TARGET = rustTargetTriple;
          nativeBuildInputs = [muslPkgs.stdenv.cc];
        }
        // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslPkgs.stdenv.cc}/bin/cc";
        }
        // pkgs.lib.optionalAttrs (system == "aarch64-linux") {
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = "${muslPkgs.stdenv.cc}/bin/cc";
        };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      staticBin = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
          doCheck = false;
        });
    in
      pkgs.dockerTools.buildLayeredImage {
        name = "agemon";
        tag = "latest";
        maxLayers = 5;
        contents = [
          pkgs.dockerTools.caCertificates
          staticBin
        ];
        config = {
          Entrypoint = ["/bin/agemon"];
        };
      };
  in
    {
      packages = forAllSystems (system:
        {
          default = mkPackage system;
        }
        // nixpkgs.lib.optionalAttrs (nixpkgs.lib.hasSuffix "-linux" system) {
          docker = mkDockerImage system;
        });

      homeManagerModules.default = {
        config,
        lib,
        pkgs,
        ...
      }: let
        cfg = config.services.agemon;
      in {
        options.services.agemon = {
          enable = lib.mkEnableOption "agemon service";

          package = lib.mkOption {
            type = lib.types.package;
            default = self.packages.${pkgs.system}.default;
            description = "The agemon package to use.";
          };

          interval = lib.mkOption {
            type = lib.types.int;
            default = 15;
            description = "Interval between metric collections in seconds.";
          };

          remoteWriteUrl = lib.mkOption {
            type = lib.types.str;
            default = "http://localhost:9090/api/v1/write";
            description = "Prometheus remote write endpoint URL.";
          };

          username = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "Username for Basic authentication.";
          };

          passwordFile = lib.mkOption {
            type = lib.types.nullOr lib.types.path;
            default = null;
            description = "Path to file containing password for Basic authentication.";
          };

          topProcesses = lib.mkOption {
            type = lib.types.int;
            default = 10;
            description = "Number of top processes to report by CPU and memory (0 to disable).";
          };

          hostname = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "Value of the hostname label. Default is the system hostname.";
          };
        };

        config = lib.mkIf cfg.enable {
          systemd.user.services.agemon = {
            Unit = {
              Description = "Agent Monitoring - push system metrics to Prometheus";
              After = ["network.target"];
            };

            Service = {
              Type = "simple";
              EnvironmentFile = lib.optionals (cfg.passwordFile != null) [
                cfg.passwordFile
              ];
              ExecStart = let
                args =
                  [
                    "--interval"
                    (toString cfg.interval)
                    "--remote-write-url"
                    cfg.remoteWriteUrl
                    "--top-processes"
                    (toString cfg.topProcesses)
                  ]
                  ++ lib.optionals (cfg.username != null) [
                    "--username"
                    cfg.username
                  ]
                  ++ lib.optionals (cfg.hostname != null) [
                    "--hostname"
                    cfg.hostname
                  ];
              in "${cfg.package}/bin/agemon ${lib.escapeShellArgs args}";
              Restart = "on-failure";
              RestartSec = 5;
            };

            Install = {
              WantedBy = ["default.target"];
            };
          };
        };
      };
    }
    // flake-utils.lib.eachDefaultSystem (system: let
      overlays = [(import rust-overlay)];
      pkgs = import nixpkgs {
        inherit system overlays;
      };

      rustToolchainDev = pkgs.rust-bin.stable.latest.default.override {
        extensions = ["rust-src" "clippy"];
      };
    in {
      devShells.default = pkgs.mkShell {
        inputsFrom = [self.packages.${system}.default];
        buildInputs = [
          rustToolchainDev
          pkgs.rust-analyzer
          pkgs.cargo-watch
          pkgs.cargo-edit
        ];

        RUST_SRC_PATH = rustToolchainDev + "/lib/rustlib/src/rust/library";

        shellHook = ''
          echo "🚀 agemon development environment loaded!"
          echo "Available commands:"
          echo "  cargo run    - Run the application"
          echo "  cargo watch  - Watch for changes and rebuild"
          echo "  cargo test   - Run tests"
          echo ""
        '';
      };

      apps.default = {
        type = "app";
        program = "${self.packages.${system}.default}/bin/agemon";
      };
    });
}
