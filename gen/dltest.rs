use std::io::Read;
use std::time::Duration;
fn main() {
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(120)).build().unwrap();
    for url in [
        "https://cdn.modrinth.com/data/i6fiqm5y/versions/rh4p4ixM/Platform-neoforge-1.21.1-1.4.2.jar",
        "https://edge.forgecdn.net/files/6182/596/SereneSeasons-neoforge-1.21.1-10.1.0.3.jar",
    ] {
        let mut resp = client.get(url).header("User-Agent", "mcman/0.2 (local minecraft server mod manager)").send().unwrap();
        println!("{url}\n  status {}", resp.status());
        let mut total = 0usize; let mut buf = vec![0u8; 262144];
        loop { buf.clear(); match resp.read(&mut buf) { Ok(0) => break, Ok(n) => total += n, Err(e) => { println!("  read error: {e}"); break } } }
        println!("  bytes {total}");
    }
}
