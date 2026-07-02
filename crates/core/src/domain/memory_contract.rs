use std::collections::BTreeSet;

use thiserror::Error;

use super::{Confidence, MemoryContractId, MemoryScope};

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryContract {
    id: MemoryContractId,
    scope: MemoryScope,
    name: String,
    source: String,
    ast: MemoryContractAst,
}

impl MemoryContract {
    /// Parses a memory contract DSL document into a typed AST.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryContractError`] when the DSL is malformed.
    pub fn parse(
        id: MemoryContractId,
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, MemoryContractError> {
        Self::parse_scoped(
            id,
            MemoryScope::new("default", "default")
                .map_err(|error| MemoryContractError::InvalidPolicy(error.to_string()))?,
            name,
            source,
        )
    }

    /// Parses a scoped memory contract DSL document into a typed AST.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryContractError`] when the DSL is malformed.
    pub fn parse_scoped(
        id: MemoryContractId,
        scope: MemoryScope,
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, MemoryContractError> {
        let source = source.into();
        let ast = MemoryContractParser::new(&source).parse()?;
        Ok(Self {
            id,
            scope,
            name: non_empty(name.into(), "contract name")?,
            source,
            ast,
        })
    }

    /// Builds validated runtime policy from the parsed contract.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryContractError`] when policy references are invalid.
    pub fn compile(&self) -> Result<CompiledMemoryPolicy, MemoryContractError> {
        CompiledMemoryPolicy::compile(self.ast.clone())
    }

    #[must_use]
    pub const fn id(&self) -> &MemoryContractId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub const fn ast(&self) -> &MemoryContractAst {
        &self.ast
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemoryContractAst {
    entity_types: Vec<EntityTypeSpec>,
    relation_types: Vec<RelationTypeSpec>,
    event_types: Vec<EventTypeSpec>,
    claim_policy: ClaimPolicySpec,
    trust_policy: TrustPolicySpec,
    review_policy: ReviewPolicySpec,
    retrieval_policies: Vec<RetrievalPolicySpec>,
    contradiction_rules: Vec<ContradictionRuleSpec>,
}

impl MemoryContractAst {
    #[must_use]
    pub fn entity_types(&self) -> &[EntityTypeSpec] {
        &self.entity_types
    }

    #[must_use]
    pub fn relation_types(&self) -> &[RelationTypeSpec] {
        &self.relation_types
    }

    #[must_use]
    pub fn event_types(&self) -> &[EventTypeSpec] {
        &self.event_types
    }

    #[must_use]
    pub const fn claim_policy(&self) -> &ClaimPolicySpec {
        &self.claim_policy
    }

    #[must_use]
    pub const fn trust_policy(&self) -> &TrustPolicySpec {
        &self.trust_policy
    }

    #[must_use]
    pub const fn review_policy(&self) -> &ReviewPolicySpec {
        &self.review_policy
    }

    #[must_use]
    pub fn retrieval_policies(&self) -> &[RetrievalPolicySpec] {
        &self.retrieval_policies
    }

    #[must_use]
    pub fn contradiction_rules(&self) -> &[ContradictionRuleSpec] {
        &self.contradiction_rules
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityTypeSpec {
    name: String,
    fields: Vec<FieldSpec>,
    aliases: Vec<String>,
}

impl EntityTypeSpec {
    fn new(name: String) -> Self {
        Self {
            name,
            fields: Vec::new(),
            aliases: Vec::new(),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn fields(&self) -> &[FieldSpec] {
        &self.fields
    }

    #[must_use]
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTypeSpec {
    name: String,
    fields: Vec<FieldSpec>,
}

impl EventTypeSpec {
    fn new(name: String) -> Self {
        Self {
            name,
            fields: Vec::new(),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn fields(&self) -> &[FieldSpec] {
        &self.fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    name: String,
    field_type: FieldType,
}

impl FieldSpec {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn field_type(&self) -> &FieldType {
        &self.field_type
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    String,
    Date,
    Boolean,
    Number,
    EntityRef(String),
    Enum(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationTypeSpec {
    name: String,
    from_entity: String,
    to_entity: String,
}

impl RelationTypeSpec {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn from_entity(&self) -> &str {
        &self.from_entity
    }

    #[must_use]
    pub fn to_entity(&self) -> &str {
        &self.to_entity
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimPolicySpec {
    require_source: bool,
    store_confidence: bool,
    allow_contradictions: bool,
    min_confidence: Confidence,
}

impl Default for ClaimPolicySpec {
    fn default() -> Self {
        Self {
            require_source: true,
            store_confidence: true,
            allow_contradictions: true,
            min_confidence: Confidence::new(0.0).expect("default confidence"),
        }
    }
}

impl ClaimPolicySpec {
    #[must_use]
    pub const fn require_source(&self) -> bool {
        self.require_source
    }

    #[must_use]
    pub const fn store_confidence(&self) -> bool {
        self.store_confidence
    }

    #[must_use]
    pub const fn allow_contradictions(&self) -> bool {
        self.allow_contradictions
    }

    #[must_use]
    pub const fn min_confidence(&self) -> Confidence {
        self.min_confidence
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrustPolicySpec {
    source_priority: Vec<String>,
}

impl TrustPolicySpec {
    #[must_use]
    pub fn source_priority(&self) -> &[String] {
        &self.source_priority
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewPolicySpec {
    review_conditions: Vec<String>,
}

impl ReviewPolicySpec {
    #[must_use]
    pub fn review_conditions(&self) -> &[String] {
        &self.review_conditions
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalPolicySpec {
    name: String,
    seed_from: Vec<String>,
    max_hops: Option<u32>,
    prefer_edges: Vec<String>,
    include_status: Option<String>,
    min_confidence: Option<Confidence>,
    exclude_stale: bool,
    exclude_restricted: bool,
}

impl RetrievalPolicySpec {
    fn new(name: String) -> Self {
        Self {
            name,
            seed_from: Vec::new(),
            max_hops: None,
            prefer_edges: Vec::new(),
            include_status: None,
            min_confidence: None,
            exclude_stale: false,
            exclude_restricted: false,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn seed_from(&self) -> &[String] {
        &self.seed_from
    }

    #[must_use]
    pub const fn max_hops(&self) -> Option<u32> {
        self.max_hops
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContradictionRuleSpec {
    name: String,
    applies_to: String,
    if_multiple_active_values: bool,
    resolve_by: Vec<String>,
    require_review: bool,
}

impl ContradictionRuleSpec {
    fn new(name: String) -> Self {
        Self {
            name,
            applies_to: String::new(),
            if_multiple_active_values: false,
            resolve_by: Vec::new(),
            require_review: false,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn applies_to(&self) -> &str {
        &self.applies_to
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledMemoryPolicy {
    ast: MemoryContractAst,
}

impl CompiledMemoryPolicy {
    fn compile(ast: MemoryContractAst) -> Result<Self, MemoryContractError> {
        if !ast.claim_policy.require_source {
            return Err(MemoryContractError::InvalidPolicy(
                "claim policy must require sources".to_string(),
            ));
        }
        let entity_names = ast
            .entity_types
            .iter()
            .map(|entity| entity.name.as_str())
            .collect::<BTreeSet<_>>();
        if entity_names.is_empty() {
            return Err(MemoryContractError::InvalidPolicy(
                "contract must define at least one entity".to_string(),
            ));
        }
        for relation in &ast.relation_types {
            validate_entity_ref(&entity_names, &relation.from_entity)?;
            validate_entity_ref(&entity_names, &relation.to_entity)?;
        }
        for entity in &ast.entity_types {
            for field in &entity.fields {
                if let FieldType::EntityRef(name) = &field.field_type {
                    validate_entity_ref(&entity_names, name)?;
                }
            }
        }
        for rule in &ast.contradiction_rules {
            validate_applies_to(&entity_names, rule.applies_to())?;
        }
        Ok(Self { ast })
    }

    #[must_use]
    pub fn entity_types(&self) -> &[EntityTypeSpec] {
        self.ast.entity_types()
    }

    #[must_use]
    pub fn relation_types(&self) -> &[RelationTypeSpec] {
        self.ast.relation_types()
    }

    #[must_use]
    pub const fn claim_policy(&self) -> &ClaimPolicySpec {
        self.ast.claim_policy()
    }

    #[must_use]
    pub const fn trust_policy(&self) -> &TrustPolicySpec {
        self.ast.trust_policy()
    }

    #[must_use]
    pub fn retrieval_policies(&self) -> &[RetrievalPolicySpec] {
        self.ast.retrieval_policies()
    }

    #[must_use]
    pub fn contradiction_rules(&self) -> &[ContradictionRuleSpec] {
        self.ast.contradiction_rules()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemoryContractError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("{0}")]
    InvalidPolicy(String),
    #[error("unknown entity type {0}")]
    UnknownEntityType(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Entity(usize),
    Relation(usize),
    Event(usize),
    ClaimPolicy,
    TrustPolicy,
    ReviewPolicy,
    RetrievalPolicy(usize),
    ContradictionRule(usize),
}

struct MemoryContractParser<'a> {
    source: &'a str,
    ast: MemoryContractAst,
    section: Option<Section>,
    subsection: Option<String>,
    pending_key: Option<String>,
}

impl<'a> MemoryContractParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            ast: MemoryContractAst::default(),
            section: None,
            subsection: None,
            pending_key: None,
        }
    }

    fn parse(mut self) -> Result<MemoryContractAst, MemoryContractError> {
        for (line_index, raw_line) in self.source.lines().enumerate() {
            let line_number = line_index + 1;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let indent = raw_line.chars().take_while(|ch| *ch == ' ').count();
            match indent {
                0 => self.parse_top_level(line_number, trimmed)?,
                2 => self.parse_property(line_number, trimmed)?,
                4 => self.parse_nested(line_number, trimmed)?,
                6 if trimmed.starts_with("- ") => {
                    self.parse_list_item(line_number, trimmed.trim_start_matches("- ").trim())?;
                }
                _ => {
                    return Err(parse_error(line_number, "unsupported indentation"));
                }
            }
        }
        Ok(self.ast)
    }

    fn parse_top_level(
        &mut self,
        line_number: usize,
        line: &str,
    ) -> Result<(), MemoryContractError> {
        self.subsection = None;
        self.pending_key = None;
        if let Some(name) = header_name(line, "entity ") {
            self.ast.entity_types.push(EntityTypeSpec::new(name));
            self.section = Some(Section::Entity(self.ast.entity_types.len() - 1));
        } else if let Some(name) = header_name(line, "relation ") {
            self.ast.relation_types.push(RelationTypeSpec {
                name,
                from_entity: String::new(),
                to_entity: String::new(),
            });
            self.section = Some(Section::Relation(self.ast.relation_types.len() - 1));
        } else if let Some(name) = header_name(line, "event ") {
            self.ast.event_types.push(EventTypeSpec::new(name));
            self.section = Some(Section::Event(self.ast.event_types.len() - 1));
        } else if line == "claim_policy:" {
            self.section = Some(Section::ClaimPolicy);
        } else if line == "trust_policy:" {
            self.section = Some(Section::TrustPolicy);
        } else if line == "review_policy:" {
            self.section = Some(Section::ReviewPolicy);
        } else if let Some(name) = header_name(line, "retrieval_policy ") {
            self.ast
                .retrieval_policies
                .push(RetrievalPolicySpec::new(name));
            self.section = Some(Section::RetrievalPolicy(
                self.ast.retrieval_policies.len() - 1,
            ));
        } else if let Some(name) = header_name(line, "contradiction_rule ") {
            self.ast
                .contradiction_rules
                .push(ContradictionRuleSpec::new(name));
            self.section = Some(Section::ContradictionRule(
                self.ast.contradiction_rules.len() - 1,
            ));
        } else {
            return Err(parse_error(line_number, "unknown top-level section"));
        }
        Ok(())
    }

    fn parse_property(
        &mut self,
        line_number: usize,
        line: &str,
    ) -> Result<(), MemoryContractError> {
        if line.ends_with(':') {
            self.subsection = Some(line.trim_end_matches(':').to_string());
            self.pending_key = None;
            return Ok(());
        }
        let (key, value) = split_key_value(line_number, line)?;
        self.pending_key = None;
        match self.section {
            Some(Section::Entity(index)) => {
                if self.subsection.as_deref() != Some("fields") {
                    return Err(parse_error(
                        line_number,
                        "entity property must be under fields",
                    ));
                }
                let field = parse_field(key, value, line_number)?;
                self.ast.entity_types[index].fields.push(field);
            }
            Some(Section::Relation(index)) => match key {
                "from" => self.ast.relation_types[index].from_entity = value.to_string(),
                "to" => self.ast.relation_types[index].to_entity = value.to_string(),
                _ => return Err(parse_error(line_number, "unknown relation property")),
            },
            Some(Section::Event(index)) => {
                if self.subsection.as_deref() != Some("fields") {
                    return Err(parse_error(
                        line_number,
                        "event property must be under fields",
                    ));
                }
                let field = parse_field(key, value, line_number)?;
                self.ast.event_types[index].fields.push(field);
            }
            Some(Section::ClaimPolicy) => self.parse_claim_policy(line_number, key, value)?,
            Some(Section::RetrievalPolicy(index)) => {
                self.parse_retrieval_property(line_number, index, key, value)?;
            }
            Some(Section::ContradictionRule(index)) => {
                self.parse_contradiction_property(line_number, index, key, value)?;
            }
            _ => {
                return Err(parse_error(
                    line_number,
                    "property is not valid in this section",
                ));
            }
        }
        Ok(())
    }

    fn parse_nested(&mut self, line_number: usize, line: &str) -> Result<(), MemoryContractError> {
        if let Some(value) = line.strip_prefix("- ") {
            self.parse_list_item(line_number, value)?;
            return Ok(());
        }
        match self.section {
            Some(Section::Entity(index)) if self.subsection.as_deref() == Some("fields") => {
                let (key, value) = split_key_value(line_number, line)?;
                let field = parse_field(key, value, line_number)?;
                self.ast.entity_types[index].fields.push(field);
                return Ok(());
            }
            Some(Section::Event(index)) if self.subsection.as_deref() == Some("fields") => {
                let (key, value) = split_key_value(line_number, line)?;
                let field = parse_field(key, value, line_number)?;
                self.ast.event_types[index].fields.push(field);
                return Ok(());
            }
            _ => {}
        }
        let (key, value) = split_key_value(line_number, line)?;
        let Some(Section::RetrievalPolicy(index)) = self.section else {
            return Err(parse_error(
                line_number,
                "nested property is not valid here",
            ));
        };
        match self.subsection.as_deref() {
            Some("expand") => {
                if key == "max_hops" {
                    self.ast.retrieval_policies[index].max_hops =
                        Some(value.parse::<u32>().map_err(|_| {
                            parse_error(line_number, "max_hops must be an integer")
                        })?);
                } else if key == "prefer_edges" && value.is_empty() {
                    self.pending_key = Some("prefer_edges".to_string());
                } else {
                    return Err(parse_error(line_number, "unknown expand property"));
                }
            }
            Some("include_claims") => match key {
                "status" => {
                    self.ast.retrieval_policies[index].include_status = Some(value.to_string());
                }
                "min_confidence" => {
                    self.ast.retrieval_policies[index].min_confidence =
                        Some(parse_confidence(value, line_number)?);
                }
                _ => return Err(parse_error(line_number, "unknown include_claims property")),
            },
            Some("exclude") => match key {
                "stale" => {
                    self.ast.retrieval_policies[index].exclude_stale =
                        parse_bool(value, line_number)?;
                }
                "restricted" => {
                    self.ast.retrieval_policies[index].exclude_restricted =
                        parse_bool(value, line_number)?;
                }
                _ => return Err(parse_error(line_number, "unknown exclude property")),
            },
            _ => {
                return Err(parse_error(
                    line_number,
                    "nested property has no subsection",
                ));
            }
        }
        Ok(())
    }

    fn parse_list_item(
        &mut self,
        line_number: usize,
        value: &str,
    ) -> Result<(), MemoryContractError> {
        match self.section {
            Some(Section::Entity(index)) => {
                if self.subsection.as_deref() == Some("aliases") {
                    self.ast.entity_types[index].aliases.push(value.to_string());
                    return Ok(());
                }
            }
            Some(Section::TrustPolicy) => {
                if self.subsection.as_deref() == Some("source_priority") {
                    self.ast
                        .trust_policy
                        .source_priority
                        .push(value.to_string());
                    return Ok(());
                }
            }
            Some(Section::ReviewPolicy) => {
                if self.subsection.as_deref() == Some("require_human_review_if") {
                    self.ast
                        .review_policy
                        .review_conditions
                        .push(value.to_string());
                    return Ok(());
                }
            }
            Some(Section::RetrievalPolicy(index)) => {
                let target = self
                    .pending_key
                    .as_deref()
                    .or(self.subsection.as_deref())
                    .unwrap_or_default();
                match target {
                    "seed_from" => {
                        self.ast.retrieval_policies[index]
                            .seed_from
                            .push(value.to_string());
                        return Ok(());
                    }
                    "prefer_edges" => {
                        self.ast.retrieval_policies[index]
                            .prefer_edges
                            .push(value.to_string());
                        return Ok(());
                    }
                    _ => {}
                }
            }
            Some(Section::ContradictionRule(index))
                if self.subsection.as_deref() == Some("resolve_by") =>
            {
                self.ast.contradiction_rules[index]
                    .resolve_by
                    .push(value.to_string());
                return Ok(());
            }
            _ => {}
        }
        Err(parse_error(line_number, "list item is not valid here"))
    }

    fn parse_claim_policy(
        &mut self,
        line_number: usize,
        key: &str,
        value: &str,
    ) -> Result<(), MemoryContractError> {
        match key {
            "require_source" => {
                self.ast.claim_policy.require_source = parse_bool(value, line_number)?;
            }
            "store_confidence" => {
                self.ast.claim_policy.store_confidence = parse_bool(value, line_number)?;
            }
            "allow_contradictions" => {
                self.ast.claim_policy.allow_contradictions = parse_bool(value, line_number)?;
            }
            "min_confidence" => {
                self.ast.claim_policy.min_confidence = parse_confidence(value, line_number)?;
            }
            _ => return Err(parse_error(line_number, "unknown claim_policy property")),
        }
        Ok(())
    }

    fn parse_retrieval_property(
        &mut self,
        line_number: usize,
        _index: usize,
        key: &str,
        value: &str,
    ) -> Result<(), MemoryContractError> {
        match key {
            "seed_from" | "expand" | "include_claims" | "exclude" if value.is_empty() => {
                self.subsection = Some(key.to_string());
                self.pending_key = None;
                Ok(())
            }
            _ => Err(parse_error(
                line_number,
                "unknown retrieval_policy property",
            )),
        }
    }

    fn parse_contradiction_property(
        &mut self,
        line_number: usize,
        index: usize,
        key: &str,
        value: &str,
    ) -> Result<(), MemoryContractError> {
        match key {
            "applies_to" => self.ast.contradiction_rules[index].applies_to = value.to_string(),
            "if_multiple_active_values" => {
                self.ast.contradiction_rules[index].if_multiple_active_values =
                    parse_bool(value, line_number)?;
            }
            "resolve_by" if value.is_empty() => {
                self.subsection = Some("resolve_by".to_string());
            }
            "require_review" => {
                self.ast.contradiction_rules[index].require_review =
                    parse_bool(value, line_number)?;
            }
            _ => {
                return Err(parse_error(
                    line_number,
                    "unknown contradiction_rule property",
                ));
            }
        }
        Ok(())
    }
}

fn header_name(line: &str, prefix: &str) -> Option<String> {
    line.strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(':'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn split_key_value(line_number: usize, line: &str) -> Result<(&str, &str), MemoryContractError> {
    let Some((key, value)) = line.split_once(':') else {
        return Err(parse_error(line_number, "expected key: value"));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(parse_error(line_number, "key cannot be empty"));
    }
    Ok((key, value.trim()))
}

fn parse_field(
    name: &str,
    value: &str,
    line_number: usize,
) -> Result<FieldSpec, MemoryContractError> {
    Ok(FieldSpec {
        name: name.to_string(),
        field_type: parse_field_type(value, line_number)?,
    })
}

fn parse_field_type(value: &str, line_number: usize) -> Result<FieldType, MemoryContractError> {
    match value {
        "string" => Ok(FieldType::String),
        "date" => Ok(FieldType::Date),
        "boolean" => Ok(FieldType::Boolean),
        "number" => Ok(FieldType::Number),
        _ if value.starts_with("enum[") && value.ends_with(']') => {
            let variants = value
                .trim_start_matches("enum[")
                .trim_end_matches(']')
                .split(',')
                .map(str::trim)
                .filter(|variant| !variant.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if variants.is_empty() {
                return Err(parse_error(
                    line_number,
                    "enum must include at least one value",
                ));
            }
            Ok(FieldType::Enum(variants))
        }
        _ if is_identifier(value) => Ok(FieldType::EntityRef(value.to_string())),
        _ => Err(parse_error(line_number, "unknown field type")),
    }
}

fn parse_bool(value: &str, line_number: usize) -> Result<bool, MemoryContractError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(parse_error(line_number, "expected boolean")),
    }
}

fn parse_confidence(value: &str, line_number: usize) -> Result<Confidence, MemoryContractError> {
    let value = value
        .parse::<f64>()
        .map_err(|_| parse_error(line_number, "confidence must be a number"))?;
    Confidence::new(value).map_err(|error| parse_error(line_number, &error.to_string()))
}

fn validate_entity_ref(
    entity_names: &BTreeSet<&str>,
    name: &str,
) -> Result<(), MemoryContractError> {
    if entity_names.contains(name) {
        Ok(())
    } else {
        Err(MemoryContractError::UnknownEntityType(name.to_string()))
    }
}

fn validate_applies_to(
    entity_names: &BTreeSet<&str>,
    applies_to: &str,
) -> Result<(), MemoryContractError> {
    let Some((entity, field)) = applies_to.split_once('.') else {
        return Err(MemoryContractError::InvalidPolicy(format!(
            "contradiction rule target {applies_to} must use Entity.field"
        )));
    };
    if field.trim().is_empty() {
        return Err(MemoryContractError::InvalidPolicy(format!(
            "contradiction rule target {applies_to} must include a field"
        )));
    }
    validate_entity_ref(entity_names, entity)
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn non_empty(value: String, field: &'static str) -> Result<String, MemoryContractError> {
    if value.trim().is_empty() {
        return Err(MemoryContractError::EmptyField { field });
    }
    Ok(value)
}

fn parse_error(line: usize, message: &str) -> MemoryContractError {
    MemoryContractError::Parse {
        line,
        message: message.to_string(),
    }
}
