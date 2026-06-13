#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    capsulet_api::runtime::run().await
}
