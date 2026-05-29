use capsulet_core::{ComponentDescriptor, ComponentKind};

fn main() {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Worker,
        "leases queued job runs and coordinates execution",
    );
    println!("{}", descriptor.banner());
}
