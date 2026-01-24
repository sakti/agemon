fn main() -> anyhow::Result<()> {
    println!("Agent monitoring");

    // println!("------------------");
    // println!(
    //     "{}",
    //     get_url_content("https://saktidwicahyono.name/healthcheck")?
    // );
    // println!("{}", get_url_content(&get_vector_download_url())?);
    // println!("------------------");

    println!(
        "User's Name            whoami::realname():    {}",
        whoami::realname(),
    );
    println!(
        "Device's Platform      whoami::platform():    {}",
        whoami::platform(),
    );
    println!(
        "Device's CPU Arch      whoami::arch():        {}",
        whoami::arch(),
    );

    Ok(())
}
