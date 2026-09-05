#[tokio::main]
async fn main() -> anyhow::Result<()> {
    capsulet_graph_worker::run().await
}
