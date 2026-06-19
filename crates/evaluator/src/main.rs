use capsulet_core::{ComponentDescriptor, ComponentKind};
use std::{thread, time::Duration};

fn main() {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Evaluator,
        "evaluates automation conditions and creates durable runs",
    );
    println!("{}", descriptor.banner());
    loop {
        thread::sleep(Duration::from_mins(1));
    }
}
