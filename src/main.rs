use std::env;

use meth::Meth;

fn main() -> bins_gui::eframe::Result<()>
{
    let _meth = Meth::new();
    dotenvy::dotenv().ok();

    let api_key_id = env::var("KALSHI_API_KEY_ID").expect("Missing KALSHI_API_KEY_ID");
    let priv_key_path =
        env::var("KALSHI_PRIVATE_KEY_PATH").expect("Missing KALSHI_PRIVATE_KEY_PATH");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to construct tokio runtime");

    // Offload all the heavy lifting to the GUI crate
    bins_gui::run_desktop_app(api_key_id, priv_key_path, rt)
}
