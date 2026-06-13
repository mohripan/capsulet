#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    capsulet_worker::runtime::run().await
}
