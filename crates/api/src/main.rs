#[tokio::main]
async fn main() -> anyhow::Result<()> {
    capsulet_api::runtime::run().await
}
