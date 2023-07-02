use xshell::{Shell, cmd};

fn main() -> anyhow::Result<()> {
    println!("Agent monitoring setup");

    let sh = Shell::new()?;

    sh.change_dir("/");
    cmd!(sh, "pwd").run()?;
    cmd!(sh, "ls -lah").run()?;

    Ok(())
}
