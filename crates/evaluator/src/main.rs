use capsulet_core::{ComponentDescriptor, ComponentKind};

fn main() {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Evaluator,
        "evaluates automation conditions and creates durable runs",
    );
    println!("{}", descriptor.banner());
}
