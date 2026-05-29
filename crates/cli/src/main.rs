use capsulet_core::{ComponentDescriptor, ComponentKind};

fn main() {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Cli,
        "operator and developer command-line interface",
    );
    println!("{}", descriptor.banner());
}
