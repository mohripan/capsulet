//! Whether a certificate covers a contract.
//!
//! This is the question a boundary is really asking, and until now nothing
//! computed it. A certificate carries `contracts: Vec<Identifier>`, and the gate
//! checked membership in that list — a list written by whoever assembled the
//! certificate, about itself. A run that discharged one spelling obligation
//! could name any contract it liked and cross a boundary protecting a different
//! one.
//!
//! The information to answer properly was already there. A [`Contract`] declares
//! the obligations it comprises, [`Definition::contract`] resolves one, and each
//! [`Obligation`] on a certificate records which contract it belongs to. What
//! was missing is that the gate only ever received the definition's *digest*,
//! so it could not look any of it up.
//!
//! **Coverage is about presence, not outcome.** Every obligation the contract
//! declares must appear on the certificate, attributed to that contract.
//! Whether each one was discharged, waived, assumed or left open is what the
//! verdict says, under the mode the run was decided in. Deciding it again here
//! would be a second, quieter copy of that rule, and the two would drift.

use crate::correctness::certificate::CertificateBody;
use crate::correctness::obligation::Contract;
use crate::definition::Definition;
use crate::id::Identifier;

/// What a certificate has to say about one contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    /// Every obligation the contract declares appears, attributed to it.
    Complete,
    /// The definition does not declare this contract, so no certificate of it
    /// can cover the contract and there is nothing to enumerate. Fails closed:
    /// a policy naming a contract the definition never had is a mismatch, not
    /// an open door.
    ContractNotInDefinition,
    /// The contract's obligations that the certificate does not account for.
    /// Never empty — a complete cover is [`Coverage::Complete`].
    Missing(Vec<Identifier>),
}

impl Coverage {
    /// Whether the contract is fully covered.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// What `certificate` covers of `contract`, as `definition` declares it.
#[must_use]
pub fn coverage(
    definition: &Definition,
    contract: &Identifier,
    certificate: &CertificateBody,
) -> Coverage {
    let Some(declared) = definition.contract(contract) else {
        return Coverage::ContractNotInDefinition;
    };
    coverage_of(declared, certificate)
}

/// What `certificate` covers of an already-resolved contract.
#[must_use]
pub fn coverage_of(contract: &Contract, certificate: &CertificateBody) -> Coverage {
    let missing: Vec<Identifier> = contract
        .obligations
        .iter()
        .filter(|statement| !accounted_for(certificate, &contract.id, &statement.id))
        .map(|statement| statement.id.clone())
        .collect();

    if missing.is_empty() {
        Coverage::Complete
    } else {
        Coverage::Missing(missing)
    }
}

/// Whether the certificate says anything about this obligation *of this
/// contract*.
///
/// The contract has to match. An obligation with the right identifier attributed
/// to a different contract is a different obligation that happens to share a
/// name, and counting it would reintroduce the hole by the back door. This is
/// the first code anywhere to read [`crate::Obligation::contract`], which until
/// now was written to the database and consulted by nothing.
fn accounted_for(
    certificate: &CertificateBody,
    contract: &Identifier,
    statement: &Identifier,
) -> bool {
    certificate
        .obligations
        .iter()
        .any(|obligation| &obligation.contract == contract && &obligation.statement.id == statement)
}
