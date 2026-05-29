use capsulet_core::{ComponentDescriptor, ComponentKind};

fn main() {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Api,
        "control plane api for automations, jobs, logs, and artifacts",
    );
    println!("{}", descriptor.banner());
}
