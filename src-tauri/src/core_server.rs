use codexswitch_lib::core;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut addr = "127.0.0.1:3900".to_string();
    let args: Vec<String> = std::env::args().collect();
    if let Some(port_idx) = args.iter().position(|arg| arg == "--port") {
        if let Some(port) = args.get(port_idx + 1) {
            addr = format!("127.0.0.1:{port}");
        }
    }

    let state = core::SharedState::new().await?;
    println!("codexswitch local core listening on http://{addr}");
    core::serve(state, &addr).await?;
    Ok(())
}
