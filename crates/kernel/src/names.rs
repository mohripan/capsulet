//! Other names for an entity, as a reviewer approved them.
//!
//! A citation compares names as text, so "International Business Machines" is not
//! found in "IBM acquired Red Hat". What may bridge that is reviewed data and nothing
//! else. A resolution with status `Confirmed` is a review: somebody looked at the
//! evidence and said these two entities are one. The `aliases` fields on an entity
//! and a canonical entity are not, because they are written straight from API
//! requests, so they do not widen what a citation matches. Neither does anything with
//! a score: an embedding may propose a candidate resolution, and only the review makes
//! it a name.
//!
//! A group is pinned in the snapshot a decision is made against, so the decision can
//! be re-made offline. Finding a term under another name is recorded on the
//! certificate, with the resolutions it relied on, because a citation that held only
//! because somebody said these are the same company should say so.

use std::collections::{BTreeMap, BTreeSet};

use capsulet_core::{Entity, EntityResolution, EntityResolutionStatus};

use crate::text;

/// The most names one group may hold. Past this a group is cut rather than refused:
/// fewer names can only make a citation match less.
pub const MAX_NAMES_PER_GROUP: usize = 32;

/// Names a reviewer joined, and the resolutions that joined them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameGroup {
    names: Vec<String>,
    resolutions: Vec<String>,
}

impl NameGroup {
    /// A group, when there is one: at least two different names, joined by at least
    /// one resolution.
    ///
    /// Names and resolutions are sorted and deduplicated, so the same review always
    /// pins the same bytes. Two names that fold to the same text are one name, because
    /// a citation could not tell them apart anyway.
    #[must_use]
    pub fn new(
        names: impl IntoIterator<Item = String>,
        resolutions: impl IntoIterator<Item = String>,
    ) -> Option<Self> {
        let mut by_fold: BTreeMap<String, String> = BTreeMap::new();
        for name in names {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                by_fold
                    .entry(text::fold(trimmed))
                    .or_insert_with(|| trimmed.to_string());
            }
        }
        let mut names: Vec<String> = by_fold.into_values().collect();
        names.sort();
        names.truncate(MAX_NAMES_PER_GROUP);

        let resolutions: Vec<String> = resolutions
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        (names.len() >= 2 && !resolutions.is_empty()).then_some(Self { names, resolutions })
    }

    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    #[must_use]
    pub fn resolutions(&self) -> &[String] {
        &self.resolutions
    }

    /// Whether `term` is one of these names, under the citation text relation.
    #[must_use]
    pub fn contains(&self, term: &str) -> bool {
        let wanted = text::fold(term.trim());
        !wanted.is_empty() && self.names.iter().any(|name| text::fold(name) == wanted)
    }
}

/// The groups a project's reviewers have established.
///
/// Only a `Confirmed` resolution joins anything. Its entity's own name is what joins:
/// the `aliases` on the entity and on the canonical entity are not reviewed, and stay
/// out. A resolution in another scope than its entity joins nothing.
#[must_use]
pub fn confirmed_name_groups(
    resolutions: &[EntityResolution],
    entities: &[Entity],
) -> Vec<NameGroup> {
    let mut joined: BTreeMap<&str, (Vec<String>, Vec<String>)> = BTreeMap::new();
    for resolution in resolutions {
        if resolution.status() != EntityResolutionStatus::Confirmed {
            continue;
        }
        let Some(entity) = entities.iter().find(|entity| {
            entity.id() == resolution.entity_id() && entity.scope() == resolution.scope()
        }) else {
            continue;
        };
        let group = joined
            .entry(resolution.canonical_entity_id().as_str())
            .or_default();
        group.0.push(entity.name().to_string());
        group.1.push(resolution.id().as_str().to_string());
    }
    joined
        .into_values()
        .filter_map(|(names, resolutions)| NameGroup::new(names, resolutions))
        .collect()
}

#[cfg(test)]
mod tests {
    use capsulet_core::{
        CanonicalEntityId, Confidence, EntityId, EntityResolutionId, EvidenceId, MemoryScope,
        MemorySubgraphId,
    };

    use super::*;

    fn scope() -> MemoryScope {
        MemoryScope::new("acme", "prod").expect("scope")
    }

    fn entity(id: &str, name: &str, aliases: &[&str]) -> Entity {
        Entity::new(
            EntityId::new(id).expect("id"),
            scope(),
            "organization",
            name,
            aliases.iter().map(ToString::to_string).collect(),
        )
        .expect("entity")
    }

    fn resolution(
        id: &str,
        entity: &str,
        canonical: &str,
        status: EntityResolutionStatus,
    ) -> EntityResolution {
        EntityResolution::new(
            EntityResolutionId::new(id).expect("id"),
            scope(),
            MemorySubgraphId::new("subgraph").expect("id"),
            EntityId::new(entity).expect("id"),
            CanonicalEntityId::new(canonical).expect("id"),
            Confidence::new(1.0).expect("confidence"),
            status,
            vec![EvidenceId::new("ev").expect("id")],
        )
        .expect("resolution")
    }

    fn ibm() -> Vec<Entity> {
        vec![
            entity("e_ibm", "IBM", &[]),
            entity("e_long", "International Business Machines", &[]),
        ]
    }

    #[test]
    fn two_confirmed_resolutions_to_one_canonical_entity_join_their_names() {
        let groups = confirmed_name_groups(
            &[
                resolution("r1", "e_ibm", "c_ibm", EntityResolutionStatus::Confirmed),
                resolution("r2", "e_long", "c_ibm", EntityResolutionStatus::Confirmed),
            ],
            &ibm(),
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].names(),
            ["IBM", "International Business Machines"]
        );
        assert_eq!(groups[0].resolutions(), ["r1", "r2"]);
    }

    #[test]
    fn a_candidate_or_rejected_resolution_joins_nothing() {
        for status in [
            EntityResolutionStatus::Candidate,
            EntityResolutionStatus::Rejected,
        ] {
            let groups = confirmed_name_groups(
                &[
                    resolution("r1", "e_ibm", "c_ibm", EntityResolutionStatus::Confirmed),
                    resolution("r2", "e_long", "c_ibm", status),
                ],
                &ibm(),
            );
            assert!(groups.is_empty(), "{status:?} must not make a name");
        }
    }

    #[test]
    fn an_alias_nobody_reviewed_is_not_a_name() {
        let entities = [
            entity("e_ibm", "IBM", &["Big Blue"]),
            entity(
                "e_long",
                "International Business Machines",
                &["Big Blue Inc"],
            ),
        ];
        let groups = confirmed_name_groups(
            &[
                resolution("r1", "e_ibm", "c_ibm", EntityResolutionStatus::Confirmed),
                resolution("r2", "e_long", "c_ibm", EntityResolutionStatus::Confirmed),
            ],
            &entities,
        );
        assert_eq!(groups.len(), 1);
        assert!(!groups[0].contains("Big Blue"));
    }

    #[test]
    fn a_single_name_is_not_a_group() {
        assert!(NameGroup::new(["IBM".to_string()], ["r1".to_string()]).is_none());
        assert!(
            NameGroup::new(["IBM".to_string(), "ibm".to_string()], ["r1".to_string()]).is_none(),
            "names that fold to the same text are one name"
        );
        assert!(
            NameGroup::new(["IBM".to_string(), "Big Blue".to_string()], Vec::new()).is_none(),
            "a group with no review behind it is not a group"
        );
    }

    #[test]
    fn the_same_review_pins_the_same_bytes() {
        let one = NameGroup::new(
            ["b".to_string(), "a".to_string()],
            ["r2".to_string(), "r1".to_string()],
        );
        let two = NameGroup::new(
            ["a".to_string(), "b".to_string()],
            ["r1".to_string(), "r2".to_string(), "r1".to_string()],
        );
        assert_eq!(one, two);
    }

    #[test]
    fn a_group_is_bounded() {
        let names = (0..100).map(|index| format!("name-{index:03}"));
        let group = NameGroup::new(names, ["r".to_string()]).expect("a group");
        assert_eq!(group.names().len(), MAX_NAMES_PER_GROUP);
    }

    #[test]
    fn a_name_matches_under_the_citation_text_relation() {
        let group = NameGroup::new(
            [
                "International Business Machines".to_string(),
                "IBM".to_string(),
            ],
            ["r".to_string()],
        )
        .expect("a group");
        assert!(group.contains("international business machines"));
        assert!(!group.contains("International"));
        assert!(!group.contains(""));
    }
}
