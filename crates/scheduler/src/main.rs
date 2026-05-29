use capsulet_core::{ComponentDescriptor, ComponentKind};

fn main() {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Scheduler,
        "scans scheduled triggers and requests automation evaluation",
    );
    println!("{}", descriptor.banner());
}
