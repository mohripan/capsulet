use capsulet_core::{ComponentDescriptor, ComponentKind};

fn main() {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Runner,
        "execution boundary for Kubernetes Job and future runner backends",
    );
    println!("{}", descriptor.banner());
}
