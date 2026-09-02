//! What a node may reach and what it may do, and the cases where saying so
//! matters.

use capsulet_ir::capability::{Capability, CapabilityError, Grant};
use capsulet_ir::effect::{Crossing, Idempotency, ProtectedBoundary, Reversibility};
use capsulet_ir::node::NodeError;
use capsulet_ir::{
    CapabilitySet, Digest, Effect, EffectKind, Identifier, Node, NodeKind, ProviderBinding,
    ResourceBudget,
};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("test identifiers are well formed")
}

fn model_grant() -> Capability {
    Capability {
        id: id("openai-small"),
        grant: Grant::ModelProvider {
            provider: id("ollama"),
            models: vec!["qwen3:4b".to_string()],
        },
    }
}

fn publish_grant() -> Capability {
    Capability {
        id: id("github-pull-requests"),
        grant: Grant::Network {
            hosts: vec!["api.github.com".to_string()],
        },
    }
}

fn publish_effect() -> Effect {
    Effect {
        id: id("open-pull-request"),
        kind: EffectKind::Publication,
        target: "github.com/mohripan/capsulet".to_string(),
        capability: id("github-pull-requests"),
        idempotency: Idempotency::Keyed {
            key_source: "run_id".to_string(),
        },
        reversibility: Reversibility::Reversible {
            compensation: id("close-pull-request"),
        },
    }
}

fn effect_node() -> Node {
    Node {
        id: id("publish"),
        name: "Open the pull request".to_string(),
        kind: NodeKind::Effect,
        capabilities: vec![id("github-pull-requests")],
        effects: vec![publish_effect()],
        budget: ResourceBudget {
            wall_ms: 30_000,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 1,
        },
        provider: None,
        sub_workflow: None,
    }
}

fn granted() -> CapabilitySet {
    CapabilitySet::new(vec![model_grant(), publish_grant()]).expect("grants are distinct")
}

#[test]
fn a_well_formed_effect_node_is_accepted() {
    assert_eq!(effect_node().check(&granted()), Ok(()));
}

#[test]
fn an_ungranted_capability_is_not_a_capability() {
    let mut node = effect_node();
    node.capabilities = vec![id("kubernetes-admin")];
    node.effects[0].capability = id("kubernetes-admin");

    assert_eq!(
        node.check(&granted()),
        Err(NodeError::Capability(CapabilityError::NotGranted {
            node: id("publish"),
            capability: id("kubernetes-admin"),
        }))
    );
}

#[test]
fn an_effect_without_an_authorising_capability_is_refused() {
    let mut node = effect_node();
    // The definition grants it, but this node never claimed it.
    node.capabilities = vec![];

    let error = node
        .check(&granted())
        .expect_err("the effect is unauthorised");
    assert!(
        error.to_string().contains("open-pull-request"),
        "the failure should name the effect, found: {error}"
    );
}

#[test]
fn a_proposer_may_not_publish() {
    let mut node = effect_node();
    node.kind = NodeKind::Proposer;
    node.capabilities = vec![id("github-pull-requests")];
    node.budget.tokens = 1_000;

    assert_eq!(
        node.check(&granted()),
        Err(NodeError::EffectKindNotAllowed {
            node: id("publish"),
            kind: "proposer",
            effect: "publication",
        })
    );
}

#[test]
fn a_pure_computation_may_not_declare_any_effect() {
    let mut node = effect_node();
    node.kind = NodeKind::PureComputation;
    // Drop the capability so the effect rule is what fails, not the earlier
    // rule that a pure computation may not hold network access either.
    node.capabilities = vec![];

    assert!(matches!(
        node.check(&granted()),
        Err(NodeError::EffectNotAllowed { .. })
    ));
}

#[test]
fn a_verifier_may_not_reach_a_model() {
    let node = Node {
        id: id("run-tests"),
        name: "Run the named tests".to_string(),
        kind: NodeKind::Verifier,
        capabilities: vec![id("openai-small")],
        effects: vec![],
        budget: ResourceBudget::deterministic(600_000),
        provider: None,
        sub_workflow: None,
    };

    assert_eq!(
        node.check(&granted()),
        Err(NodeError::Capability(
            CapabilityError::NotPermittedForKind {
                node: id("run-tests"),
                kind: "verifier",
                capability: id("openai-small"),
                grant: "a model provider",
            }
        ))
    );
}

#[test]
fn an_effect_node_must_declare_the_effect_it_exists_for() {
    let mut node = effect_node();
    node.effects = vec![];

    assert_eq!(
        node.check(&granted()),
        Err(NodeError::EffectRequired {
            node: id("publish"),
            kind: "effect node",
        })
    );
}

#[test]
fn a_provider_binding_must_spend_a_capability_the_node_holds() {
    let node = Node {
        id: id("propose-patch"),
        name: "Propose a patch".to_string(),
        kind: NodeKind::Proposer,
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget {
            wall_ms: 60_000,
            tokens: 8_000,
            cost_micro_units: 500,
            effect_count: 0,
        },
        provider: Some(ProviderBinding {
            capability: id("openai-small"),
            selection: "qwen3:4b".to_string(),
        }),
        sub_workflow: None,
    };

    assert_eq!(
        node.check(&granted()),
        Err(NodeError::Capability(CapabilityError::NotGranted {
            node: id("propose-patch"),
            capability: id("openai-small"),
        }))
    );
}

#[test]
fn a_keyed_effect_must_name_its_key_source() {
    let mut node = effect_node();
    node.effects[0].idempotency = Idempotency::Keyed {
        key_source: "  ".to_string(),
    };

    let error = node
        .check(&granted())
        .expect_err("a keyed effect needs a key");
    assert!(error.to_string().contains("idempotency key"));
}

#[test]
fn a_node_that_performs_effects_needs_an_effect_budget() {
    let mut node = effect_node();
    node.budget.effect_count = 0;

    assert_eq!(
        node.check(&granted()),
        Err(NodeError::EmptyBudget {
            node: id("publish"),
            resource: "effects",
        })
    );
}

#[test]
fn a_duplicate_grant_is_refused_rather_than_shadowed() {
    assert_eq!(
        CapabilitySet::new(vec![publish_grant(), publish_grant()]),
        Err(CapabilityError::Duplicate {
            capability: id("github-pull-requests"),
        })
    );
}

#[test]
fn a_nested_scope_may_narrow_capabilities_but_not_widen_them() {
    let parent = granted();
    let narrowed = CapabilitySet::new(vec![publish_grant()]).expect("grants are distinct");
    assert_eq!(narrowed.check_narrows(&parent), Ok(()));

    let widened = CapabilitySet::new(vec![
        publish_grant(),
        Capability {
            id: id("cluster-admin"),
            grant: Grant::Filesystem {
                paths: vec!["/".to_string()],
                write: true,
            },
        },
    ])
    .expect("grants are distinct");
    assert_eq!(
        widened.check_narrows(&parent),
        Err(CapabilityError::WidensParent {
            capability: id("cluster-admin"),
        })
    );

    // Same name, different grant: also a widening, and the sneakier one.
    let substituted = CapabilitySet::new(vec![Capability {
        id: id("github-pull-requests"),
        grant: Grant::Network {
            hosts: vec!["*".to_string()],
        },
    }])
    .expect("grants are distinct");
    assert_eq!(
        substituted.check_narrows(&parent),
        Err(CapabilityError::WidensParent {
            capability: id("github-pull-requests"),
        })
    );
}

#[test]
fn a_container_image_grant_carries_its_digest() {
    // Recorded as a type-level fact: the grant cannot be constructed without a
    // digest, so it cannot name whatever the registry serves today.
    let grant = Grant::ContainerImage {
        image: "ghcr.io/mohripan/capsulet-verifier".to_string(),
        digest: Digest::of(b"an image"),
    };
    assert_eq!(grant.kind_name(), "a container image");
}

#[test]
fn a_protected_boundary_names_what_it_protects() {
    let boundary = ProtectedBoundary {
        id: id("publish-boundary"),
        node: id("publish"),
        crossing: Crossing::Effect {
            effect: id("open-pull-request"),
        },
        description: "Opening a pull request against the customer repository".to_string(),
    };

    match boundary.crossing {
        Crossing::Effect { effect } => assert_eq!(effect, id("open-pull-request")),
        Crossing::TrustTransition { .. } => panic!("this boundary protects an effect"),
    }
}

#[test]
fn identifiers_reject_padding_and_stray_characters() {
    assert!(Identifier::parse("").is_err());
    assert!(Identifier::parse(" publish").is_err());
    assert!(Identifier::parse("publish now").is_err());
    assert!(Identifier::parse("publish/pull-request:v1").is_ok());
}
