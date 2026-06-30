use engram_core::MemoryService;
use engram_domain::{RetrievalRequest, WriteMemoryRequest};
use engram_store_memory::InMemoryMemoryService;
use futures::executor::block_on;

fn write_fixture() -> WriteMemoryRequest {
    serde_json::from_str(include_str!(
        "../../../../contracts/v1/examples/write-memory-request.json"
    ))
    .expect("accepted write-memory fixture should deserialize")
}

fn retrieval_fixture() -> RetrievalRequest {
    serde_json::from_str(include_str!(
        "../../../../contracts/v1/examples/retrieval-request.json"
    ))
    .expect("accepted retrieval fixture should deserialize")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    block_on(async {
        let service = InMemoryMemoryService::new();
        let write = service.write_memory(write_fixture()).await?;
        let context = service.retrieve(retrieval_fixture()).await?;

        println!("wrote memory {}", write.record.id);
        println!("retrieved {} item(s)", context.items.len());
        if let Some(item) = context.items.first() {
            println!("top result: {}", item.content);
        }

        Ok(())
    })
}
