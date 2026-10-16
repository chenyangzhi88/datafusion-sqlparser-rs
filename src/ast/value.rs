// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

#[cfg(not(feature = "std"))]
use alloc::{string::String, sync::Arc, vec::Vec};
#[cfg(feature = "std")]
use std::sync::Arc;

use core::{
    fmt,
    ops::{Deref, DerefMut},
};

/// Opaque lifetime of an allocation admitted by the caller's existing budget.
/// It has no SQL, compiler, credential or charset authority.
#[derive(Clone)]
pub struct SqlSourceAdmission {
    #[cfg(feature = "std")]
    guard: Arc<dyn Send + Sync>,
}

impl SqlSourceAdmission {
    #[cfg(feature = "std")]
    /// Retains the caller's existing allocation lease without granting SQL authority.
    /// The lease is released when its final shared guard is dropped.
    pub fn new<T: Send + Sync + 'static>(guard: Arc<T>) -> Self {
        Self { guard }
    }

    #[cfg(feature = "std")]
    /// Moves the same allocation guard across a parser-neutral boundary.
    /// This does not allocate, copy the payload or grant SQL authority.
    pub fn into_shared_guard(self) -> Arc<dyn Send + Sync> {
        self.guard
    }

    #[cfg(feature = "std")]
    /// Wraps an existing shared allocation lifetime without changing its owner.
    /// The guard conveys no SQL, compiler, credential or charset authority.
    pub fn from_shared_guard(guard: Arc<dyn Send + Sync>) -> Self {
        Self { guard }
    }

    #[cfg(not(feature = "std"))]
    /// Creates the no-std admission token; the caller remains responsible for
    /// accounting and retaining any external allocation lease.
    pub fn new() -> Self {
        Self {}
    }
}

impl fmt::Debug for SqlSourceAdmission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SqlSourceAdmission")
    }
}

/// The one immutable original byte allocation and its actual admission lease.
/// Adopted Vec capacity, not just its length, must be admitted before this call.
pub struct SqlSourceOwner {
    bytes: Vec<u8>,
    // Drop the original byte allocation before its retained admission lease.
    _admission: SqlSourceAdmission,
}

impl SqlSourceOwner {
    /// Adopts one immutable byte allocation and its already acquired lease.
    /// The caller must admit the Vec's capacity and shared-owner header before
    /// calling; this constructor does not reserve memory or validate SQL.
    pub fn adopt(bytes: Vec<u8>, admission: SqlSourceAdmission) -> Arc<Self> {
        Arc::new(Self {
            bytes,
            _admission: admission,
        })
    }

    /// Shared physical owner header, excluding the separately admitted Vec capacity.
    pub fn owner_allocation_bytes() -> usize {
        core::mem::size_of::<Self>() + 2 * core::mem::size_of::<usize>()
    }

    /// Borrows original bytes without decoding or producing compiler values.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the adopted Vec capacity, including unused admitted storage.
    pub fn capacity(&self) -> usize {
        self.bytes.capacity()
    }

    /// Creates an attached, half-open byte occurrence retaining this owner.
    /// Rejects reversed or out-of-bounds ranges; an empty in-bounds range is valid.
    pub fn occurrence(
        self: &Arc<Self>,
        range: core::ops::Range<usize>,
    ) -> Result<SqlSourceOccurrence, String> {
        if range.start > range.end || range.end > self.bytes.len() {
            return Err("SQL source occurrence is outside its admitted owner".into());
        }
        Ok(SqlSourceOccurrence {
            owner: Some(self.clone()),
            start: range.start,
            end: range.end,
            canonical_digest: None,
        })
    }
}

impl fmt::Debug for SqlSourceOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqlSourceOwner")
            .field("length", &self.bytes.len())
            .field("capacity", &self.bytes.capacity())
            .finish_non_exhaustive()
    }
}

/// Checked original byte range. Serialized occurrences contain ONLY ranges;
/// catalog readers must explicitly reattach their one admitted source owner.
/// No serialized reservation, duplicated entire SQL or implicit rebinding.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SqlSourceOccurrence {
    #[cfg_attr(feature = "serde", serde(skip))]
    owner: Option<Arc<SqlSourceOwner>>,
    start: usize,
    end: usize,
    #[cfg_attr(feature = "serde", serde(skip))]
    canonical_digest: Option<crate::tokenizer::MySqlCanonicalDigestReceipt>,
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for SqlSourceOccurrence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Transport {
            start: usize,
            end: usize,
        }
        let value = Transport::deserialize(deserializer)?;
        if value.start > value.end {
            return Err(serde::de::Error::custom(
                "invalid detached SQL source range",
            ));
        }
        Ok(Self {
            owner: None,
            start: value.start,
            end: value.end,
            canonical_digest: None,
        })
    }
}

impl SqlSourceOccurrence {
    /// Borrows facts sealed by this occurrence's successful raw statement
    /// grammar. They are not serialized, inherited by subranges, or an
    /// execution/PSI publication credential; consumers still validate the
    /// original owner, occurrence, parse role and current controls.
    pub fn canonical_digest(&self) -> Option<&crate::tokenizer::MySqlCanonicalDigestReceipt> {
        self.canonical_digest.as_ref()
    }

    pub(crate) fn attach_canonical_digest(
        &mut self,
        digest: crate::tokenizer::MySqlCanonicalDigestReceipt,
    ) {
        self.canonical_digest = Some(digest);
    }

    /// Returns the half-open offsets into the original owner's byte allocation.
    /// Detached offsets alone do not establish source admission or byte identity.
    pub fn range(&self) -> core::ops::Range<usize> {
        self.start..self.end
    }

    /// Borrows the attached owner, or returns `None` after range-only deserialization.
    pub fn owner(&self) -> Option<&Arc<SqlSourceOwner>> {
        self.owner.as_ref()
    }

    /// Borrows the checked original byte slice, failing for a detached occurrence.
    pub fn bytes(&self) -> Result<&[u8], String> {
        self.owner
            .as_ref()
            .and_then(|owner| owner.bytes.get(self.range()))
            .ok_or_else(|| "SQL source occurrence requires checked owner reattachment".into())
    }

    /// Attaches this range after checking it against the supplied owner.
    /// The catalog caller must supply the matching original source and its
    /// admission; bounds checking does not compare historical SQL contents.
    pub fn reattach(&mut self, owner: &Arc<SqlSourceOwner>) -> Result<(), String> {
        let checked = owner.occurrence(self.range())?;
        *self = checked;
        Ok(())
    }

    /// Creates a checked sub-occurrence using offsets relative to this slice.
    /// Requires attachment and rejects out-of-bounds ranges or offset overflow.
    pub fn slice(&self, relative: core::ops::Range<usize>) -> Result<Self, String> {
        let bytes = self.bytes()?;
        if relative.start > relative.end || relative.end > bytes.len() {
            return Err("SQL source subrange is outside its occurrence".into());
        }
        let start = self
            .start
            .checked_add(relative.start)
            .ok_or_else(|| String::from("SQL source offset overflow"))?;
        let end = self
            .start
            .checked_add(relative.end)
            .ok_or_else(|| String::from("SQL source offset overflow"))?;
        self.owner
            .as_ref()
            .ok_or_else(|| String::from("SQL source has no owner"))?
            .occurrence(start..end)
    }
}

impl PartialEq for SqlSourceOccurrence {
    fn eq(&self, other: &Self) -> bool {
        self.range() == other.range()
    }
}
impl Eq for SqlSourceOccurrence {}
impl PartialOrd for SqlSourceOccurrence {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SqlSourceOccurrence {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.start, self.end).cmp(&(other.start, other.end))
    }
}
impl core::hash::Hash for SqlSourceOccurrence {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(&(self.start, self.end), state);
    }
}

/// Real source character bytes, not a Rust decoded mirror or Compiler value.
/// An unchanged literal borrows the original owner; genuinely unescaped bytes
/// have their own caller-admitted allocation. Original occurrence stays exact.
#[derive(Clone, Debug)]
pub struct MySqlText {
    semantic: MySqlTextSemantic,
    segments: MySqlTextSegments,
    role: MySqlTextRole,
}

/// Native grammar role, not a conversion result or source-to-Compiler grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MySqlTextRole {
    /// Literal semantic bytes before the consuming expression's charset policy.
    Literal,
    /// TEXT_STRING_sys bytes before the actual system-text consumer conversion.
    System,
    /// File-name bytes whose filesystem conversion and access policy belong to
    /// the consumer; this role does not authorize reading or writing a file.
    FileSystem,
    /// Text from a grammar role requiring native character validation by its
    /// consumer; the role itself does not certify conversion has taken place.
    Validated,
    /// Projection-alias bytes before native alias/name policy is applied.
    Alias,
}

/// One actual literal reduction, indexing the sole semantic byte buffer.
/// Original source and introducer facts are not inferred from concatenated text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MySqlTextSegment {
    semantic_start: usize,
    semantic_end: usize,
    source: SqlSourceOccurrence,
    charset: String,
    quote: char,
    introducer: Option<String>,
    binary_literal: Option<crate::tokenizer::MySqlHexLiteralKind>,
}

impl MySqlTextSegment {
    /// Records one literal reduction over a shared semantic byte buffer.
    /// Checks range ordering, attached original source, nonempty charset and
    /// quote syntax. The enclosing text checks the complete buffer partition.
    /// `introducer` is the actual source fact, not a deduction from byte contents.
    pub fn new(
        semantic_range: core::ops::Range<usize>,
        source: SqlSourceOccurrence,
        charset: String,
        quote: char,
        introducer: Option<String>,
    ) -> Result<Self, String> {
        if semantic_range.start > semantic_range.end
            || charset.is_empty()
            || !matches!(quote, '\'' | '"' | '`' | '\0')
        {
            return Err("invalid MySQL literal segment facts".into());
        }
        source.bytes()?;
        Ok(Self {
            semantic_start: semantic_range.start,
            semantic_end: semantic_range.end,
            source,
            charset,
            quote,
            introducer,
            binary_literal: None,
        })
    }
    /// Returns this segment's half-open offsets in the sole semantic buffer.
    pub fn semantic_range(&self) -> core::ops::Range<usize> {
        self.semantic_start..self.semantic_end
    }
    /// Borrows the exact original source occurrence for this literal reduction.
    pub fn source(&self) -> &SqlSourceOccurrence {
        &self.source
    }
    /// Returns the source charset fact supplied for this segment, not a decoded
    /// UTF-8 classification of its semantic bytes.
    pub fn charset(&self) -> &str {
        &self.charset
    }
    /// Returns the literal delimiter; `\0` denotes the unquoted role.
    pub fn quote(&self) -> char {
        self.quote
    }
    /// Returns the explicit source introducer, if the literal had one.
    pub fn introducer(&self) -> Option<&str> {
        self.introducer.as_deref()
    }
    fn semantic_key(
        &self,
    ) -> (
        usize,
        usize,
        &str,
        char,
        Option<&str>,
        Option<crate::tokenizer::MySqlHexLiteralKind>,
    ) {
        (
            self.semantic_start,
            self.semantic_end,
            &self.charset,
            self.quote,
            self.introducer(),
            self.binary_literal,
        )
    }
}

#[derive(Clone, Debug)]
enum MySqlTextSegments {
    Single([MySqlTextSegment; 1]),
    /// Caller admits Vec capacity, segment string capacities and owner header
    /// before allocating. This lease follows the same immutable text entity.
    Adjacent(Arc<MySqlTextSegmentOwner>),
}

#[derive(Clone, Debug)]
struct MySqlTextSegmentOwner {
    parts: Vec<MySqlTextSegment>,
    admission: Option<SqlSourceAdmission>,
}

fn check_mysql_text_segments(parts: &[MySqlTextSegment], length: usize) -> Result<(), String> {
    if parts.is_empty() {
        return Err("MySQL text has no literal segment".into());
    }
    let mut end = 0;
    let mut source_end = 0;
    for part in parts {
        if part.semantic_start != end
            || part.semantic_end < end
            || part.semantic_end > length
            || part.charset.is_empty()
            || !matches!(part.quote, '\'' | '"' | '`' | '\0')
            || part.source.start > part.source.end
            || part.source.start < source_end
        {
            return Err("MySQL text segments do not partition its semantic buffer".into());
        }
        end = part.semantic_end;
        source_end = part.source.end;
    }
    if end != length {
        return Err("MySQL text segments leave unclaimed bytes".into());
    }
    Ok(())
}

#[derive(Clone, Debug)]
enum MySqlTextSemantic {
    Admitted(SqlSourceOccurrence),
    /// Catalog bytes with restored ownership, never a fresh lexer or alias issuance.
    Reattached(SqlSourceOccurrence),
    /// Same semantic owner after the actual native declaration adapter bound
    /// its connection/binary byte domain. Original occurrence remains in segments.
    Declaration(SqlSourceOccurrence),
    /// The same admitted semantic owner after the genuine System reduction.
    System(SqlSourceOccurrence),
    /// The same owner after actual output-alias acceptance. This logical
    /// relationship is distinct from general System text and IDENT authority.
    AliasSystem(SqlSourceOccurrence),
    /// A decoded catalog transport value, not admitted execution source.
    Detached(Vec<u8>),
    /// Durable declaration facts without runtime Source/admission proof.
    DetachedDeclaration(Vec<u8>),
    /// Durable System bytes without the original adapter issuance proof.
    DetachedSystem(Vec<u8>),
}

impl MySqlText {
    /// Same admitted semantic allocation of an actual lexer identifier.
    /// Borrowing it does not clone segment strings or grant parser authority.
    pub(crate) fn canonical_identifier_occurrence(&self) -> Option<&SqlSourceOccurrence> {
        match &self.semantic {
            MySqlTextSemantic::Admitted(source)
            | MySqlTextSemantic::Declaration(source)
            | MySqlTextSemantic::System(source) => Some(source),
            MySqlTextSemantic::AliasSystem(_)
            | MySqlTextSemantic::Reattached(_)
            | MySqlTextSemantic::Detached(_)
            | MySqlTextSemantic::DetachedDeclaration(_)
            | MySqlTextSemantic::DetachedSystem(_) => None,
        }
    }

    /// Header for the one shared adjacent-segment allocation, in addition to
    /// Vec and String capacities charged by its builder/catalog caller.
    pub fn segment_owner_allocation_bytes() -> usize {
        core::mem::size_of::<MySqlTextSegmentOwner>() + 2 * core::mem::size_of::<usize>()
    }
    /// Constructs a single-segment literal from attached semantic and original
    /// source occurrences. Unescaped bytes may borrow the original owner;
    /// transformed bytes must already have their own admitted owner.
    pub fn from_occurrence(
        semantic: SqlSourceOccurrence,
        source: SqlSourceOccurrence,
        charset: String,
        quote: char,
        introducer: Option<String>,
    ) -> Result<Self, String> {
        let length = semantic.bytes()?.len();
        let segment = MySqlTextSegment::new(0..length, source, charset, quote, introducer)?;
        Ok(Self {
            semantic: MySqlTextSemantic::Admitted(semantic),
            segments: MySqlTextSegments::Single([segment]),
            role: MySqlTextRole::Literal,
        })
    }
    /// Combines adjacent literal reductions without copying their semantic bytes.
    /// Segments must exactly partition that buffer in order and share one attached
    /// original source owner. The caller admits segment/string capacities and
    /// the shared-owner header before transferring `parts` and its lease.
    pub fn from_segments(
        semantic: SqlSourceOccurrence,
        parts: Vec<MySqlTextSegment>,
        admission: SqlSourceAdmission,
    ) -> Result<Self, String> {
        check_mysql_text_segments(&parts, semantic.bytes()?.len())?;
        let owner = parts[0]
            .source
            .owner()
            .ok_or_else(|| String::from("detached literal source"))?;
        for part in &parts {
            part.source.bytes()?;
            if !part
                .source
                .owner()
                .is_some_and(|other| Arc::ptr_eq(owner, other))
            {
                return Err("adjacent literals do not share their actual source owner".into());
            }
        }
        Ok(Self {
            semantic: MySqlTextSemantic::Admitted(semantic),
            segments: MySqlTextSegments::Adjacent(Arc::new(MySqlTextSegmentOwner {
                parts,
                admission: Some(admission),
            })),
            role: MySqlTextRole::Literal,
        })
    }
    /// Bind this actual single declaration reduction. A grammar role alone
    /// cannot call the bound-state reader; the adapter supplies admitted
    /// semantic bytes and the matching original owner before replacement.
    pub fn bind_declaration(
        &mut self,
        semantic: SqlSourceOccurrence,
        charset: String,
        expected: &Arc<SqlSourceOwner>,
    ) -> Result<(), String> {
        if self.segments().len() != 1
            || self.introducer().is_some()
            || matches!(
                self.semantic,
                MySqlTextSemantic::System(_)
                    | MySqlTextSemantic::Reattached(_)
                    | MySqlTextSemantic::AliasSystem(_)
                    | MySqlTextSemantic::DetachedSystem(_)
            )
        {
            return Err("declaration requires one unintroduced non-System literal".into());
        }
        let source = self.source().clone();
        if !source
            .owner()
            .is_some_and(|owner| Arc::ptr_eq(owner, expected))
        {
            return Err("declaration belongs to a foreign original Source".into());
        }
        source.bytes()?;
        let length = semantic.bytes()?.len();
        let mut part = MySqlTextSegment::new(0..length, source, charset, self.quote(), None)?;
        part.binary_literal = self.binary_literal();
        self.segments = MySqlTextSegments::Single([part]);
        self.semantic = MySqlTextSemantic::Declaration(semantic);
        Ok(())
    }
    /// Actual admitted declaration state; detached serde facts and with_role
    /// do not confer this proof. Used only inside native datatype declarations.
    pub fn declaration_bytes(&self) -> Result<&[u8], String> {
        let MySqlTextSemantic::Declaration(semantic) = &self.semantic else {
            return Err("native declaration is not adapter-bound".into());
        };
        if self.segments().len() != 1 || self.introducer().is_some() {
            return Err("invalid native declaration grammar".into());
        }
        self.source().bytes()?;
        semantic.bytes()
    }
    /// Bind one actual single System reduction with its once-converted,
    /// pre-admitted semantic owner and the exact original Source identity.
    pub fn bind_system(
        &mut self,
        semantic: SqlSourceOccurrence,
        expected: &Arc<SqlSourceOwner>,
    ) -> Result<(), String> {
        self.bind_system_role(semantic, expected, MySqlTextRole::System)
    }

    /// The actual projection-alias reduction supplies accepted system bytes.
    /// with_role, ordinary System text and detached transport cannot substitute
    /// for this same-original-Source handback.
    pub fn bind_alias_system(
        &mut self,
        semantic: SqlSourceOccurrence,
        expected: &Arc<SqlSourceOwner>,
    ) -> Result<(), String> {
        self.bind_system_role(semantic, expected, MySqlTextRole::Alias)
    }

    fn bind_system_role(
        &mut self,
        semantic: SqlSourceOccurrence,
        expected: &Arc<SqlSourceOwner>,
        role: MySqlTextRole,
    ) -> Result<(), String> {
        if self.role != role
            || !matches!(self.semantic, MySqlTextSemantic::Admitted(_))
            || self.segments().len() != 1
            || self.introducer().is_some()
            || self.binary_literal().is_some()
            || !self
                .source()
                .owner()
                .is_some_and(|owner| Arc::ptr_eq(owner, expected))
        {
            return Err("System reduction lost its single original Source".into());
        }
        let source = self.source().clone();
        source.bytes()?;
        let length = semantic.bytes()?.len();
        let part = MySqlTextSegment::new(
            0..length,
            source,
            String::from("utf8mb3"),
            self.quote(),
            None,
        )?;
        self.segments = MySqlTextSegments::Single([part]);
        self.semantic = if role == MySqlTextRole::Alias {
            MySqlTextSemantic::AliasSystem(semantic)
        } else {
            MySqlTextSemantic::System(semantic)
        };
        Ok(())
    }

    /// Borrow the actually accepted alias, not an identifier/Field capability.
    /// Serde intentionally restores only detached bytes, never this relation.
    pub fn alias_system_bytes(&self) -> Result<&[u8], String> {
        let MySqlTextSemantic::AliasSystem(semantic) = &self.semantic else {
            return Err("output alias is not adapter-bound".into());
        };
        if self.role != MySqlTextRole::Alias
            || self.segments().len() != 1
            || self.introducer().is_some()
            || self.binary_literal().is_some()
        {
            return Err("invalid output alias grammar".into());
        }
        self.source().bytes()?;
        semantic.bytes()
    }

    /// Actual adapter-bound System bytes, not role/serde/shape permission.
    pub fn system_bytes(&self) -> Result<&[u8], String> {
        let MySqlTextSemantic::System(semantic) = &self.semantic else {
            return Err("System text is not adapter-bound".into());
        };
        if self.role != MySqlTextRole::System
            || self.segments().len() != 1
            || self.introducer().is_some()
            || self.binary_literal().is_some()
        {
            return Err("invalid System text grammar".into());
        }
        self.source().bytes()?;
        semantic.bytes()
    }

    /// Durable System grammar facts alone do not grant runtime admission.
    pub fn is_system(&self) -> bool {
        matches!(
            self.semantic,
            MySqlTextSemantic::System(_) | MySqlTextSemantic::DetachedSystem(_)
        )
    }

    /// Reports durable declaration grammar facts without granting admission.
    pub fn is_declaration(&self) -> bool {
        matches!(
            self.semantic,
            MySqlTextSemantic::Declaration(_) | MySqlTextSemantic::DetachedDeclaration(_)
        )
    }

    /// Records only the actual lexer binary production consumed by native
    /// text_string grammar. The original source occurrence remains unchanged.
    pub fn with_binary_literal(
        mut self,
        kind: crate::tokenizer::MySqlHexLiteralKind,
    ) -> Result<Self, String> {
        let MySqlTextSegments::Single(parts) = &mut self.segments else {
            return Err("binary declaration cannot be an adjacent literal".into());
        };
        if parts[0].charset != "binary" || parts[0].introducer.is_some() {
            return Err("binary declaration has an incompatible source domain".into());
        }
        parts[0].binary_literal = Some(kind);
        Ok(self)
    }
    /// Actual HEX_NUM/BIN_NUM lexical provenance, never inferred from displayed text.
    pub fn binary_literal(&self) -> Option<crate::tokenizer::MySqlHexLiteralKind> {
        self.segments()[0].binary_literal
    }
    /// Mark already admitted binary semantic bytes in the original owner.
    pub fn bind_binary_declaration(
        &mut self,
        expected: &Arc<SqlSourceOwner>,
    ) -> Result<(), String> {
        if self.binary_literal().is_none() || self.charset() != "binary" {
            return Err("declaration has no actual binary lexical production".into());
        }
        if !self
            .source()
            .owner()
            .is_some_and(|owner| Arc::ptr_eq(owner, expected))
        {
            return Err("binary declaration belongs to a foreign original Source".into());
        }
        self.source().bytes()?;
        let old = core::mem::replace(&mut self.semantic, MySqlTextSemantic::Detached(Vec::new()));
        self.semantic = match old {
            MySqlTextSemantic::Admitted(value) | MySqlTextSemantic::Declaration(value) => {
                MySqlTextSemantic::Declaration(value)
            }
            other => {
                self.semantic = other;
                return Err("binary declaration requires runtime admission".into());
            }
        };
        Ok(())
    }

    /// Returns the native grammar role, independently of conversion or admission.
    pub fn role(&self) -> MySqlTextRole {
        self.role
    }
    /// Assigns the grammar role without decoding bytes or granting compiler,
    /// filesystem or execution capabilities.
    pub fn with_role(mut self, role: MySqlTextRole) -> Self {
        self.role = role;
        self
    }
    /// Borrows the checked ordered reductions indexing the sole semantic buffer.
    pub fn segments(&self) -> &[MySqlTextSegment] {
        match &self.segments {
            MySqlTextSegments::Single(parts) => parts,
            MySqlTextSegments::Adjacent(owner) => &owner.parts,
        }
    }
    /// Borrows one segment's semantic bytes, rejecting a detached value or an
    /// invalid ordinal/range rather than inventing a decoded fallback.
    pub fn segment_bytes(&self, ordinal: usize) -> Result<&[u8], String> {
        let part = self
            .segments()
            .get(ordinal)
            .ok_or_else(|| String::from("literal segment ordinal is out of range"))?;
        self.bytes()?
            .get(part.semantic_range())
            .ok_or_else(|| String::from("literal segment is outside semantic bytes"))
    }
    /// Borrows admitted semantic bytes; deserialized transport bytes require
    /// explicit catalog reattachment and admission before they are accessible.
    pub fn bytes(&self) -> Result<&[u8], String> {
        match &self.semantic {
            MySqlTextSemantic::Admitted(value)
            | MySqlTextSemantic::Reattached(value)
            | MySqlTextSemantic::Declaration(value)
            | MySqlTextSemantic::AliasSystem(value)
            | MySqlTextSemantic::System(value) => value.bytes(),
            MySqlTextSemantic::Detached(_)
            | MySqlTextSemantic::DetachedDeclaration(_)
            | MySqlTextSemantic::DetachedSystem(_) => {
                Err("MySQL text requires catalog admission".into())
            }
        }
    }
    fn semantic_bytes(&self) -> &[u8] {
        match &self.semantic {
            MySqlTextSemantic::Admitted(value)
            | MySqlTextSemantic::Reattached(value)
            | MySqlTextSemantic::Declaration(value)
            | MySqlTextSemantic::AliasSystem(value)
            | MySqlTextSemantic::System(value) => value.bytes().expect("constructed checked text"),
            MySqlTextSemantic::Detached(bytes)
            | MySqlTextSemantic::DetachedDeclaration(bytes)
            | MySqlTextSemantic::DetachedSystem(bytes) => bytes,
        }
    }
    /// Borrows the first literal's original occurrence. For adjacent literals,
    /// inspect [`Self::segments`] for every contributing original range.
    pub fn source(&self) -> &SqlSourceOccurrence {
        self.segments()[0].source()
    }
    /// Returns the first segment's charset fact. Adjacent reductions retain
    /// their individual charsets in [`Self::segments`].
    pub fn charset(&self) -> &str {
        self.segments()[0].charset()
    }
    /// Returns the first segment's delimiter, not a rendered whole-text quote.
    pub fn quote(&self) -> char {
        self.segments()[0].quote()
    }
    /// Returns the first segment's explicit source introducer, if present.
    pub fn introducer(&self) -> Option<&str> {
        self.segments()[0].introducer()
    }
    /// Records an introducer on a single reduction before adjacent concatenation.
    /// Rejects already combined segments so their source facts are not rewritten.
    pub fn with_introducer(mut self, introducer: String) -> Result<Self, String> {
        match &mut self.segments {
            MySqlTextSegments::Single(parts) => parts[0].introducer = Some(introducer),
            MySqlTextSegments::Adjacent(_) => {
                return Err("introducer must precede adjacent literal reduction".into());
            }
        }
        Ok(self)
    }
    /// Exact retained transport capacities and possible shared-owner clone
    /// headers needed by catalog reattachment. This reports allocation cost,
    /// not source or compiler authority; overflow is explicit.
    pub fn semantic_allocation_bytes(&self) -> Option<usize> {
        let semantic = match &self.semantic {
            MySqlTextSemantic::Admitted(_)
            | MySqlTextSemantic::Reattached(_)
            | MySqlTextSemantic::Declaration(_)
            | MySqlTextSemantic::AliasSystem(_)
            | MySqlTextSemantic::System(_) => 0,
            MySqlTextSemantic::Detached(bytes)
            | MySqlTextSemantic::DetachedDeclaration(bytes)
            | MySqlTextSemantic::DetachedSystem(bytes) => bytes
                .capacity()
                .checked_add(SqlSourceOwner::owner_allocation_bytes())?,
        };
        let segment = match &self.segments {
            MySqlTextSegments::Single(_) => 0,
            MySqlTextSegments::Adjacent(owner) => owner
                .parts
                .capacity()
                .checked_mul(core::mem::size_of::<MySqlTextSegment>())?
                .checked_add(Self::segment_owner_allocation_bytes())?,
        };
        self.segments()
            .iter()
            .try_fold(semantic.checked_add(segment)?, |cost, part| {
                cost.checked_add(part.charset.capacity())?
                    .checked_add(part.introducer.as_ref().map_or(0, String::capacity))
            })
    }

    /// Reattaches detached original ranges and admits the semantic transport bytes.
    /// Checks segment partition and source bounds. The caller must provide the
    /// matching original source and pre-admit semantic capacity plus any cloned
    /// segment/string storage; leases are retained with the resulting owners.
    /// Serialized roles/ranges never restore compiler or execution authority.
    pub fn reattach(
        &mut self,
        owner: &Arc<SqlSourceOwner>,
        semantic_admission: SqlSourceAdmission,
    ) -> Result<(), String> {
        check_mysql_text_segments(self.segments(), self.semantic_bytes().len())?;
        for part in self.segments() {
            owner.occurrence(part.source.range())?;
        }
        match &mut self.segments {
            MySqlTextSegments::Single(parts) => parts[0].source.reattach(owner)?,
            MySqlTextSegments::Adjacent(parts_owner) => {
                // Reattachment caller admits the new segment container/string
                // capacities before this potential catalog-transport clone.
                let parts_owner = Arc::make_mut(parts_owner);
                for part in &mut parts_owner.parts {
                    part.source.reattach(owner)?;
                }
                parts_owner.admission = Some(semantic_admission.clone());
            }
        }
        let old = core::mem::replace(&mut self.semantic, MySqlTextSemantic::Detached(Vec::new()));
        let (bytes, declaration) = match old {
            MySqlTextSemantic::Detached(bytes) | MySqlTextSemantic::DetachedSystem(bytes) => {
                (bytes, false)
            }
            MySqlTextSemantic::DetachedDeclaration(bytes) => (bytes, true),
            attached => {
                self.semantic = attached;
                return Ok(());
            }
        };
        let length = bytes.len();
        let semantic = SqlSourceOwner::adopt(bytes, semantic_admission);
        let occurrence = semantic.occurrence(0..length)?;
        self.semantic = if declaration {
            MySqlTextSemantic::Declaration(occurrence)
        } else {
            MySqlTextSemantic::Reattached(occurrence)
        };
        Ok(())
    }
}

impl PartialEq for MySqlText {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for MySqlText {}
impl PartialOrd for MySqlText {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MySqlText {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.semantic_bytes(), self.role)
            .cmp(&(other.semantic_bytes(), other.role))
            .then_with(|| {
                self.segments()
                    .iter()
                    .map(MySqlTextSegment::semantic_key)
                    .cmp(other.segments().iter().map(MySqlTextSegment::semantic_key))
            })
    }
}
impl core::hash::Hash for MySqlText {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(
            &(self.semantic_bytes(), self.role, self.segments().len()),
            state,
        );
        for segment in self.segments() {
            core::hash::Hash::hash(&segment.semantic_key(), state);
        }
    }
}

impl fmt::Display for MySqlText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display is presentation only. It must never manufacture a lossy,
        // hex or sentinel source string for bytes that do not fit fmt::str.
        // Preflight every contribution before emitting any prefix. source()
        // remains the first segment for native Item result-name semantics.
        for segment in self.segments() {
            core::str::from_utf8(segment.source().bytes().map_err(|_| fmt::Error)?)
                .map_err(|_| fmt::Error)?;
        }
        for (ordinal, segment) in self.segments().iter().enumerate() {
            if ordinal != 0 {
                f.write_str(" ")?;
            }
            let bytes = segment.source().bytes().map_err(|_| fmt::Error)?;
            f.write_str(core::str::from_utf8(bytes).map_err(|_| fmt::Error)?)?;
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
impl Serialize for MySqlText {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("MySqlText", 5)?;
        state.serialize_field("bytes", self.semantic_bytes())?;
        state.serialize_field("segments", self.segments())?;
        state.serialize_field("role", &self.role)?;
        state.serialize_field("declaration", &self.is_declaration())?;
        state.serialize_field("system", &self.is_system())?;
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for MySqlText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Transport {
            bytes: Vec<u8>,
            segments: Vec<MySqlTextSegment>,
            role: MySqlTextRole,
            declaration: bool,
            #[serde(default)]
            system: bool,
        }
        let value = Transport::deserialize(deserializer)?;
        if value.system && value.declaration {
            return Err(serde::de::Error::custom(
                "conflicting System/declaration transport states",
            ));
        }
        check_mysql_text_segments(&value.segments, value.bytes.len())
            .map_err(serde::de::Error::custom)?;
        Ok(Self {
            semantic: if value.system {
                MySqlTextSemantic::DetachedSystem(value.bytes)
            } else if value.declaration {
                MySqlTextSemantic::DetachedDeclaration(value.bytes)
            } else {
                MySqlTextSemantic::Detached(value.bytes)
            },
            segments: MySqlTextSegments::Adjacent(Arc::new(MySqlTextSegmentOwner {
                parts: value.segments,
                admission: None,
            })),
            role: value.role,
        })
    }
}

#[cfg(feature = "visitor")]
macro_rules! source_visit_noop {
    ($($ty:ty),* $(,)?) => {$ (
        impl crate::ast::Visit for $ty {
            fn visit<V: crate::ast::Visitor>(&self, _: &mut V) -> core::ops::ControlFlow<V::Break> { core::ops::ControlFlow::Continue(()) }
        }
        impl crate::ast::VisitMut for $ty {
            fn visit<V: crate::ast::VisitorMut>(&mut self, _: &mut V) -> core::ops::ControlFlow<V::Break> { core::ops::ControlFlow::Continue(()) }
        }
    )*};
}
#[cfg(feature = "visitor")]
source_visit_noop!(
    SqlSourceOccurrence,
    SqlCppPieceRange,
    SqlCppLedger,
    MySqlProjectionLiteral
);

/// A projection's original grammar naming fact, not a second executable
/// literal. Visitors must not rewrite/evaluate it along with the VALUE Item.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MySqlProjectionLiteral(MySqlText);

impl MySqlProjectionLiteral {
    /// Retains the original literal grammar node for projection naming only.
    /// This does not evaluate or bind its VALUE, or acquire source admission.
    pub fn new(text: MySqlText) -> Self {
        Self(text)
    }
    /// Borrows the original literal segments and their shared source owner.
    pub fn text(&self) -> &MySqlText {
        &self.0
    }
}

#[cfg(feature = "visitor")]
impl crate::ast::Visit for MySqlText {
    fn visit<V: crate::ast::Visitor>(&self, visitor: &mut V) -> core::ops::ControlFlow<V::Break> {
        visitor.pre_visit_mysql_text(self)?;
        visitor.post_visit_mysql_text(self)
    }
}
#[cfg(feature = "visitor")]
impl crate::ast::VisitMut for MySqlText {
    fn visit<V: crate::ast::VisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> core::ops::ControlFlow<V::Break> {
        visitor.pre_visit_mysql_text(self)?;
        visitor.post_visit_mysql_text(self)
    }
}

/// Indices into the single CPP piece ledger returned with a parsed statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SqlCppPieceRange {
    /// Inclusive piece index in the containing parsed statement's CPP ledger.
    pub start: usize,
    /// Exclusive piece index; offsets index pieces, not source or rendered bytes.
    pub end: usize,
}

/// A native CPP contribution, not rendered Expr text or duplicated SQL.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SqlCppPiece {
    /// Exact bytes contributed by an attached original source occurrence.
    Source(SqlSourceOccurrence),
    /// One space inserted by the native CPP comment/token boundary policy.
    /// It has no fabricated original-byte occurrence.
    Space,
}

/// One shared immutable physical CPP ledger. Projection nodes retain this
/// same owner, never a per-projection copy of the ledger or original SQL.
/// Catalog serialization deliberately does not preserve runtime admission.
#[derive(Clone, Debug)]
pub struct SqlCppLedger(Arc<SqlCppLedgerOwner>);

#[derive(Debug)]
struct SqlCppLedgerOwner {
    pieces: Vec<SqlCppPiece>,
    source_charset: &'static str,
    connection_charset: &'static str,
    _admissions: Vec<SqlSourceAdmission>,
    _container: SqlSourceAdmission,
    _owner: SqlSourceAdmission,
}

impl SqlCppLedger {
    /// Returns the shared ledger header and Arc counter allocation size.
    /// The piece and admission Vec allocations require their own caller leases.
    pub fn owner_allocation_bytes() -> usize {
        core::mem::size_of::<SqlCppLedgerOwner>() + 2 * core::mem::size_of::<usize>()
    }

    /// Adopt the original admitted Vec allocations. `owner` covers this
    /// shared header; the original leases cover pieces and retained admissions.
    pub fn adopt(
        pieces: Vec<SqlCppPiece>,
        admissions: Vec<SqlSourceAdmission>,
        container: SqlSourceAdmission,
        owner: SqlSourceAdmission,
        source_charset: &'static str,
        connection_charset: &'static str,
    ) -> Self {
        Self(Arc::new(SqlCppLedgerOwner {
            pieces,
            source_charset,
            connection_charset,
            _admissions: admissions,
            _container: container,
            _owner: owner,
        }))
    }

    /// Borrows a checked half-open range of original CPP contributions.
    /// Returns an error if the range is reversed or outside this ledger.
    pub fn pieces(&self, range: SqlCppPieceRange) -> Result<&[SqlCppPiece], String> {
        range.pieces(&self.0.pieces)
    }

    /// Tests the actual shared ledger owner, not semantic piece equality.
    pub fn has_same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Borrows every contribution from this single shared immutable ledger.
    pub fn all_pieces(&self) -> &[SqlCppPiece] {
        &self.0.pieces
    }
    /// Returns the original byte source charset recorded by the adopting caller.
    pub fn source_charset(&self) -> &'static str {
        self.0.source_charset
    }
    /// Returns the connection charset recorded for this parse's CPP conversion.
    pub fn connection_charset(&self) -> &'static str {
        self.0.connection_charset
    }
}

impl PartialEq for SqlCppLedger {
    fn eq(&self, other: &Self) -> bool {
        self.0.pieces == other.0.pieces
    }
}
impl Eq for SqlCppLedger {}
impl PartialOrd for SqlCppLedger {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SqlCppLedger {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.pieces.cmp(&other.0.pieces)
    }
}
impl core::hash::Hash for SqlCppLedger {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(&self.0.pieces, state);
    }
}

impl SqlCppPieceRange {
    /// Borrows the checked half-open piece slice from the matching statement
    /// ledger, failing when the range is reversed or outside that ledger.
    pub fn pieces<'a>(&self, ledger: &'a [SqlCppPiece]) -> Result<&'a [SqlCppPiece], String> {
        ledger
            .get(self.start..self.end)
            .ok_or_else(|| "CPP range is outside its statement ledger".into())
    }
}

#[cfg(feature = "bigdecimal")]
use bigdecimal::BigDecimal;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{ast::Ident, tokenizer::Span};
#[cfg(feature = "visitor")]
use sqlparser_derive::{Visit, VisitMut};

/// Wraps a primitive SQL [`Value`]  with its [`Span`] location
///
/// # Example: create a `ValueWithSpan` from a `Value`
/// ```
/// # use sqlparser::ast::{Value, ValueWithSpan};
/// # use sqlparser::tokenizer::{Location, Span};
/// let value = Value::SingleQuotedString(String::from("endpoint"));
/// // from line 1, column 1 to line 1, column 7
/// let span = Span::new(Location::new(1, 1), Location::new(1, 7));
/// let value_with_span = value.with_span(span);
/// ```
///
/// # Example: create a `ValueWithSpan` from a `Value` with an empty span
///
/// You can call [`Value::with_empty_span`] to create a `ValueWithSpan` with an empty span
/// ```
/// # use sqlparser::ast::{Value, ValueWithSpan};
/// # use sqlparser::tokenizer::{Location, Span};
/// let value = Value::SingleQuotedString(String::from("endpoint"));
/// let value_with_span = value.with_empty_span();
/// assert_eq!(value_with_span.span, Span::empty());
/// ```
///
/// You can also use the [`From`] trait to convert  `ValueWithSpan` to/from `Value`s
/// ```
/// # use sqlparser::ast::{Value, ValueWithSpan};
/// # use sqlparser::tokenizer::{Location, Span};
/// let value = Value::SingleQuotedString(String::from("endpoint"));
/// // converting `Value` to `ValueWithSpan` results in an empty span
/// let value_with_span: ValueWithSpan = value.into();
/// assert_eq!(value_with_span.span, Span::empty());
/// // convert back to `Value`
/// let value: Value = value_with_span.into();
/// ```
/// A `Value` paired with its source `Span` location.
#[derive(Debug, Clone, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "visitor",
    derive(Visit, VisitMut),
    visit(with = "visit_value")
)]
pub struct ValueWithSpan {
    /// The wrapped `Value`.
    pub value: Value,
    /// The source `Span` covering the token(s) that produced the value.
    pub span: Span,
}

impl PartialEq for ValueWithSpan {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Ord for ValueWithSpan {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl PartialOrd for ValueWithSpan {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(Ord::cmp(self, other))
    }
}

impl core::hash::Hash for ValueWithSpan {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl From<Value> for ValueWithSpan {
    fn from(value: Value) -> Self {
        value.with_empty_span()
    }
}

impl From<ValueWithSpan> for Value {
    fn from(value: ValueWithSpan) -> Self {
        value.value
    }
}

impl Deref for ValueWithSpan {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl DerefMut for ValueWithSpan {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Primitive SQL values such as number and string
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum Value {
    /// A real MySQL byte-source literal; never compiler-created capability.
    MySqlText(MySqlText),
    /// Numeric literal
    #[cfg(not(feature = "bigdecimal"))]
    Number(String, bool),
    #[cfg(feature = "bigdecimal")]
    /// HINT: use `test_utils::number` to make an instance of
    /// Value::Number This might help if you your tests pass locally
    /// but fail on CI with the `--all-features` flag enabled
    /// Numeric literal (uses `BigDecimal` when the `bigdecimal` feature is enabled).
    Number(BigDecimal, bool),
    /// 'string value'
    SingleQuotedString(String),
    /// Dollar-quoted string literal, e.g. `$$...$$` or `$tag$...$tag$` (Postgres syntax).
    DollarQuotedString(DollarQuotedString),
    /// Triple single quoted strings: Example '''abc'''
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleSingleQuotedString(String),
    /// Triple double quoted strings: Example """abc"""
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleDoubleQuotedString(String),
    /// e'string value' (postgres extension)
    /// See [Postgres docs](https://www.postgresql.org/docs/8.3/sql-syntax-lexical.html#SQL-SYNTAX-STRINGS)
    /// for more details.
    EscapedStringLiteral(String),
    /// u&'string value' (postgres extension)
    /// See [Postgres docs](https://www.postgresql.org/docs/current/sql-syntax-lexical.html#SQL-SYNTAX-STRINGS-UESCAPE)
    /// for more details.
    UnicodeStringLiteral(String),
    /// B'string value'
    SingleQuotedByteStringLiteral(String),
    /// B"string value"
    DoubleQuotedByteStringLiteral(String),
    /// Triple single quoted literal with byte string prefix. Example `B'''abc'''`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleSingleQuotedByteStringLiteral(String),
    /// Triple double quoted literal with byte string prefix. Example `B"""abc"""`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleDoubleQuotedByteStringLiteral(String),
    /// Single quoted literal with raw string prefix. Example `R'abc'`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    SingleQuotedRawStringLiteral(String),
    /// Double quoted literal with raw string prefix. Example `R"abc"`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    DoubleQuotedRawStringLiteral(String),
    /// Triple single quoted literal with raw string prefix. Example `R'''abc'''`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleSingleQuotedRawStringLiteral(String),
    /// Triple double quoted literal with raw string prefix. Example `R"""abc"""`
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/lexical#quoted_literals)
    TripleDoubleQuotedRawStringLiteral(String),
    /// N'string value'
    NationalStringLiteral(String),
    /// Quote delimited literal. Examples `Q'{ab'c}'`, `Q'|ab'c|'`, `Q'|ab|c|'`
    /// [Oracle](https://docs.oracle.com/en/database/oracle/oracle-database/21/sqlrf/Literals.html#GUID-1824CBAA-6E16-4921-B2A6-112FB02248DA)
    QuoteDelimitedStringLiteral(QuoteDelimitedString),
    /// "National" quote delimited literal. Examples `Q'{ab'c}'`, `Q'|ab'c|'`, `Q'|ab|c|'`
    /// [Oracle](https://docs.oracle.com/en/database/oracle/oracle-database/21/sqlrf/Literals.html#GUID-1824CBAA-6E16-4921-B2A6-112FB02248DA)
    NationalQuoteDelimitedStringLiteral(QuoteDelimitedString),
    /// X'hex value'
    HexStringLiteral(String),

    /// Double quoted string literal, e.g. `"abc"`.
    DoubleQuotedString(String),
    /// Boolean value true or false
    Boolean(bool),
    /// `NULL` value
    Null,
    /// `?` or `$` Prepared statement arg placeholder
    Placeholder(String),
}

impl ValueWithSpan {
    /// If the underlying literal is a string, regardless of quote style, returns the associated string value
    pub fn into_string(self) -> Option<String> {
        self.value.into_string()
    }
}

impl Value {
    /// If the underlying literal is a string, regardless of quote style, returns the associated string value
    pub fn into_string(self) -> Option<String> {
        match self {
            Value::SingleQuotedString(s)
            | Value::DoubleQuotedString(s)
            | Value::TripleSingleQuotedString(s)
            | Value::TripleDoubleQuotedString(s)
            | Value::SingleQuotedByteStringLiteral(s)
            | Value::DoubleQuotedByteStringLiteral(s)
            | Value::TripleSingleQuotedByteStringLiteral(s)
            | Value::TripleDoubleQuotedByteStringLiteral(s)
            | Value::SingleQuotedRawStringLiteral(s)
            | Value::DoubleQuotedRawStringLiteral(s)
            | Value::TripleSingleQuotedRawStringLiteral(s)
            | Value::TripleDoubleQuotedRawStringLiteral(s)
            | Value::EscapedStringLiteral(s)
            | Value::UnicodeStringLiteral(s)
            | Value::NationalStringLiteral(s)
            | Value::HexStringLiteral(s) => Some(s),
            Value::DollarQuotedString(s) => Some(s.value),
            Value::QuoteDelimitedStringLiteral(s) => Some(s.value),
            Value::NationalQuoteDelimitedStringLiteral(s) => Some(s.value),
            _ => None,
        }
    }

    /// Attach the provided `span` to this `Value` and return `ValueWithSpan`.
    pub fn with_span(self, span: Span) -> ValueWithSpan {
        ValueWithSpan { value: self, span }
    }

    /// Convenience for attaching an empty span to this `Value`.
    pub fn with_empty_span(self) -> ValueWithSpan {
        self.with_span(Span::empty())
    }
}

impl fmt::Display for ValueWithSpan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::MySqlText(v) => v.fmt(f),
            Value::Number(v, l) => write!(f, "{}{long}", v, long = if *l { "L" } else { "" }),
            Value::DoubleQuotedString(v) => write!(f, "\"{}\"", escape_double_quote_string(v)),
            Value::SingleQuotedString(v) => write!(f, "'{}'", escape_single_quote_string(v)),
            Value::TripleSingleQuotedString(v) => {
                write!(f, "'''{v}'''")
            }
            Value::TripleDoubleQuotedString(v) => {
                write!(f, r#""""{v}""""#)
            }
            Value::DollarQuotedString(v) => write!(f, "{v}"),
            Value::EscapedStringLiteral(v) => write!(f, "E'{}'", escape_escaped_string(v)),
            Value::UnicodeStringLiteral(v) => write!(f, "U&'{}'", escape_unicode_string(v)),
            Value::NationalStringLiteral(v) => write!(f, "N'{v}'"),
            Value::QuoteDelimitedStringLiteral(v) => v.fmt(f),
            Value::NationalQuoteDelimitedStringLiteral(v) => write!(f, "N{v}"),
            Value::HexStringLiteral(v) => write!(f, "X'{v}'"),
            Value::Boolean(v) => write!(f, "{v}"),
            Value::SingleQuotedByteStringLiteral(v) => write!(f, "B'{v}'"),
            Value::DoubleQuotedByteStringLiteral(v) => write!(f, "B\"{v}\""),
            Value::TripleSingleQuotedByteStringLiteral(v) => write!(f, "B'''{v}'''"),
            Value::TripleDoubleQuotedByteStringLiteral(v) => write!(f, r#"B"""{v}""""#),
            Value::SingleQuotedRawStringLiteral(v) => write!(f, "R'{v}'"),
            Value::DoubleQuotedRawStringLiteral(v) => write!(f, "R\"{v}\""),
            Value::TripleSingleQuotedRawStringLiteral(v) => write!(f, "R'''{v}'''"),
            Value::TripleDoubleQuotedRawStringLiteral(v) => write!(f, r#"R"""{v}""""#),
            Value::Null => write!(f, "NULL"),
            Value::Placeholder(v) => write!(f, "{v}"),
        }
    }
}

/// A dollar-quoted string literal, e.g. `$$...$$` or `$tag$...$tag$`.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct DollarQuotedString {
    /// Inner string contents.
    pub value: String,
    /// Optional tag used in the opening/closing delimiter.
    pub tag: Option<String>,
}

impl fmt::Display for DollarQuotedString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.tag {
            Some(tag) => {
                write!(f, "${}${}${}$", tag, self.value, tag)
            }
            None => {
                write!(f, "$${}$$", self.value)
            }
        }
    }
}

/// A quote delimited string literal, e.g. `Q'_abc_'`.
///
/// See [Value::QuoteDelimitedStringLiteral] and/or
/// [Value::NationalQuoteDelimitedStringLiteral].
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub struct QuoteDelimitedString {
    /// the quote start character; i.e. the character _after_ the opening `Q'`
    pub start_quote: char,
    /// the string literal value itself
    pub value: String,
    /// the quote end character; i.e. the character _before_ the closing `'`
    pub end_quote: char,
}

impl fmt::Display for QuoteDelimitedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Q'{}{}{}'", self.start_quote, self.value, self.end_quote)
    }
}

/// Represents the date/time fields used by functions like `EXTRACT`.
///
/// Each variant corresponds to a supported date/time part (for example
/// `YEAR`, `MONTH`, `DAY`, etc.). The `Custom` variant allows arbitrary
/// identifiers (e.g. dialect-specific abbreviations).
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum DateTimeField {
    /// `YEAR`
    Year,
    /// `YEARS` (plural form)
    Years,
    /// `MONTH`
    Month,
    /// `MONTHS` (plural form)
    Months,
    /// `WEEK`, optionally followed by a weekday, e.g. `WEEK(MONDAY)`.
    ///
    /// [BigQuery](https://cloud.google.com/bigquery/docs/reference/standard-sql/date_functions#extract)
    Week(Option<Ident>),
    /// `WEEKS` (plural form)
    Weeks,
    /// `DAY`
    Day,
    /// `DAYOFWEEK`
    DayOfWeek,
    /// `DAYOFYEAR`
    DayOfYear,
    /// `DAYS` (plural form)
    Days,
    /// `DATE`
    Date,
    /// `DATETIME`
    Datetime,
    /// `HOUR`
    Hour,
    /// `HOURS` (plural form)
    Hours,
    /// `MINUTE`
    Minute,
    /// `MINUTES` (plural form)
    Minutes,
    /// `SECOND`
    Second,
    /// `SECONDS` (plural form)
    Seconds,
    /// `CENTURY`
    Century,
    /// `DECADE`
    Decade,
    /// `DOW` (day of week short form)
    Dow,
    /// `DOY` (day of year short form)
    Doy,
    /// `EPOCH`
    Epoch,
    /// `ISODOW`
    Isodow,
    /// `ISOYEAR`
    Isoyear,
    /// `ISOWEEK`
    IsoWeek,
    /// `JULIAN`
    Julian,
    /// `MICROSECOND`
    Microsecond,
    /// `MICROSECONDS` (plural form)
    Microseconds,
    /// `MILLENIUM` (alternate spelling)
    Millenium,
    /// `MILLENNIUM` (alternate spelling)
    Millennium,
    /// `MILLISECOND`
    Millisecond,
    /// `MILLISECONDS` (plural form)
    Milliseconds,
    /// `NANOSECOND`
    Nanosecond,
    /// `NANOSECONDS` (plural form)
    Nanoseconds,
    /// `QUARTER`
    Quarter,
    /// `TIME`
    Time,
    /// `TIMEZONE`
    Timezone,
    /// `TIMEZONE_ABBR`
    TimezoneAbbr,
    /// `TIMEZONE_HOUR`
    TimezoneHour,
    /// `TIMEZONE_MINUTE`
    TimezoneMinute,
    /// `TIMEZONE_REGION`
    TimezoneRegion,
    /// `NODATETIME` indicates no date/time part
    NoDateTime,
    /// Arbitrary abbreviation or custom date-time part.
    ///
    /// ```sql
    /// EXTRACT(q FROM CURRENT_TIMESTAMP)
    /// ```
    /// [Snowflake](https://docs.snowflake.com/en/sql-reference/functions-date-time#supported-date-and-time-parts)
    Custom(Ident),
}

impl fmt::Display for DateTimeField {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DateTimeField::Year => write!(f, "YEAR"),
            DateTimeField::Years => write!(f, "YEARS"),
            DateTimeField::Month => write!(f, "MONTH"),
            DateTimeField::Months => write!(f, "MONTHS"),
            DateTimeField::Week(week_day) => {
                write!(f, "WEEK")?;
                if let Some(week_day) = week_day {
                    write!(f, "({week_day})")?
                }
                Ok(())
            }
            DateTimeField::Weeks => write!(f, "WEEKS"),
            DateTimeField::Day => write!(f, "DAY"),
            DateTimeField::DayOfWeek => write!(f, "DAYOFWEEK"),
            DateTimeField::DayOfYear => write!(f, "DAYOFYEAR"),
            DateTimeField::Days => write!(f, "DAYS"),
            DateTimeField::Date => write!(f, "DATE"),
            DateTimeField::Datetime => write!(f, "DATETIME"),
            DateTimeField::Hour => write!(f, "HOUR"),
            DateTimeField::Hours => write!(f, "HOURS"),
            DateTimeField::Minute => write!(f, "MINUTE"),
            DateTimeField::Minutes => write!(f, "MINUTES"),
            DateTimeField::Second => write!(f, "SECOND"),
            DateTimeField::Seconds => write!(f, "SECONDS"),
            DateTimeField::Century => write!(f, "CENTURY"),
            DateTimeField::Decade => write!(f, "DECADE"),
            DateTimeField::Dow => write!(f, "DOW"),
            DateTimeField::Doy => write!(f, "DOY"),
            DateTimeField::Epoch => write!(f, "EPOCH"),
            DateTimeField::Isodow => write!(f, "ISODOW"),
            DateTimeField::Isoyear => write!(f, "ISOYEAR"),
            DateTimeField::IsoWeek => write!(f, "ISOWEEK"),
            DateTimeField::Julian => write!(f, "JULIAN"),
            DateTimeField::Microsecond => write!(f, "MICROSECOND"),
            DateTimeField::Microseconds => write!(f, "MICROSECONDS"),
            DateTimeField::Millenium => write!(f, "MILLENIUM"),
            DateTimeField::Millennium => write!(f, "MILLENNIUM"),
            DateTimeField::Millisecond => write!(f, "MILLISECOND"),
            DateTimeField::Milliseconds => write!(f, "MILLISECONDS"),
            DateTimeField::Nanosecond => write!(f, "NANOSECOND"),
            DateTimeField::Nanoseconds => write!(f, "NANOSECONDS"),
            DateTimeField::Quarter => write!(f, "QUARTER"),
            DateTimeField::Time => write!(f, "TIME"),
            DateTimeField::Timezone => write!(f, "TIMEZONE"),
            DateTimeField::TimezoneAbbr => write!(f, "TIMEZONE_ABBR"),
            DateTimeField::TimezoneHour => write!(f, "TIMEZONE_HOUR"),
            DateTimeField::TimezoneMinute => write!(f, "TIMEZONE_MINUTE"),
            DateTimeField::TimezoneRegion => write!(f, "TIMEZONE_REGION"),
            DateTimeField::NoDateTime => write!(f, "NODATETIME"),
            DateTimeField::Custom(custom) => write!(f, "{custom}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
/// The Unicode Standard defines four normalization forms, which are intended to eliminate
/// certain distinctions between visually or functionally identical characters.
///
/// See [Unicode Normalization Forms](https://unicode.org/reports/tr15/) for details.
pub enum NormalizationForm {
    /// Canonical Decomposition, followed by Canonical Composition.
    NFC,
    /// Canonical Decomposition.
    NFD,
    /// Compatibility Decomposition, followed by Canonical Composition.
    NFKC,
    /// Compatibility Decomposition.
    NFKD,
}

impl fmt::Display for NormalizationForm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NormalizationForm::NFC => write!(f, "NFC"),
            NormalizationForm::NFD => write!(f, "NFD"),
            NormalizationForm::NFKC => write!(f, "NFKC"),
            NormalizationForm::NFKD => write!(f, "NFKD"),
        }
    }
}

pub struct EscapeQuotedString<'a> {
    string: &'a str,
    quote: char,
}

impl fmt::Display for EscapeQuotedString<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // EscapeQuotedString doesn't know which mode of escape was
        // chosen by the user. So this code must to correctly display
        // strings without knowing if the strings are already escaped
        // or not.
        //
        // If the quote symbol in the string is repeated twice, OR, if
        // the quote symbol is after backslash, display all the chars
        // without any escape. However, if the quote symbol is used
        // just between usual chars, `fmt()` should display it twice."
        //
        // The following table has examples
        //
        // | original query | mode      | AST Node                                           | serialized   |
        // | -------------  | --------- | -------------------------------------------------- | ------------ |
        // | `"A""B""A"`    | no-escape | `DoubleQuotedString(String::from("A\"\"B\"\"A"))`  | `"A""B""A"`  |
        // | `"A""B""A"`    | default   | `DoubleQuotedString(String::from("A\"B\"A"))`      | `"A""B""A"`  |
        // | `"A\"B\"A"`    | no-escape | `DoubleQuotedString(String::from("A\\\"B\\\"A"))`  | `"A\"B\"A"`  |
        // | `"A\"B\"A"`    | default   | `DoubleQuotedString(String::from("A\"B\"A"))`      | `"A""B""A"`  |
        let quote = self.quote;
        let mut previous_char = char::default();
        let mut start_idx = 0;
        let mut peekable_chars = self.string.char_indices().peekable();
        while let Some(&(idx, ch)) = peekable_chars.peek() {
            match ch {
                char if char == quote => {
                    if previous_char == '\\' {
                        // the quote is already escaped with a backslash, skip
                        peekable_chars.next();
                        continue;
                    }
                    peekable_chars.next();
                    match peekable_chars.peek() {
                        Some((_, c)) if *c == quote => {
                            // the quote is already escaped with another quote, skip
                            peekable_chars.next();
                        }
                        _ => {
                            // The quote is not escaped.
                            // Including idx in the range, so the quote at idx will be printed twice:
                            // in this call to write_str() and in the next one.
                            f.write_str(&self.string[start_idx..=idx])?;
                            start_idx = idx;
                        }
                    }
                }
                _ => {
                    peekable_chars.next();
                }
            }
            previous_char = ch;
        }
        f.write_str(&self.string[start_idx..])?;
        Ok(())
    }
}

/// Return a helper which formats `string` for inclusion inside a quoted
/// literal that uses `quote` as the delimiter.
pub fn escape_quoted_string(string: &str, quote: char) -> EscapeQuotedString<'_> {
    EscapeQuotedString { string, quote }
}

/// Preserve bare variable spelling when it can be parsed as one name.
/// Constructed names need quoting for SQL delimiters or empty dotted components.
pub(super) fn fmt_user_variable(ident: &Ident, f: &mut fmt::Formatter) -> fmt::Result {
    if ident.quote_style.is_some()
        || ident.value.split('.').all(|component| {
            !component.is_empty()
                && component.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(character, '_' | '$')
                        || (!character.is_ascii()
                            && !character.is_whitespace()
                            && !character.is_control())
                })
        })
    {
        write!(f, "@{ident}")
    } else {
        write!(f, "@`{}`", escape_quoted_string(&ident.value, '`'))
    }
}

/// Convenience wrapper for escaping strings for single-quoted literals (`'`).
pub fn escape_single_quote_string(s: &str) -> EscapeQuotedString<'_> {
    escape_quoted_string(s, '\'')
}

/// Convenience wrapper for escaping strings for double-quoted literals (`").`
pub fn escape_double_quote_string(s: &str) -> EscapeQuotedString<'_> {
    escape_quoted_string(s, '\"')
}

pub struct EscapeEscapedStringLiteral<'a>(&'a str);

impl fmt::Display for EscapeEscapedStringLiteral<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '\'' => {
                    write!(f, r#"\'"#)?;
                }
                '\\' => {
                    write!(f, r#"\\"#)?;
                }
                '\n' => {
                    write!(f, r#"\n"#)?;
                }
                '\t' => {
                    write!(f, r#"\t"#)?;
                }
                '\r' => {
                    write!(f, r#"\r"#)?;
                }
                _ => {
                    write!(f, "{c}")?;
                }
            }
        }
        Ok(())
    }
}

/// Return a helper which escapes characters for string literals that use
/// PostgreSQL-style escaped string literals (e.g. `E'...')`.
pub fn escape_escaped_string(s: &str) -> EscapeEscapedStringLiteral<'_> {
    EscapeEscapedStringLiteral(s)
}

pub struct EscapeUnicodeStringLiteral<'a>(&'a str);

impl fmt::Display for EscapeUnicodeStringLiteral<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '\'' => {
                    write!(f, "''")?;
                }
                '\\' => {
                    write!(f, r#"\\"#)?;
                }
                x if x.is_ascii() => {
                    write!(f, "{c}")?;
                }
                _ => {
                    let codepoint = c as u32;
                    // if the character fits in 32 bits, we can use the \XXXX format
                    // otherwise, we need to use the \+XXXXXX format
                    if codepoint <= 0xFFFF {
                        write!(f, "\\{codepoint:04X}")?;
                    } else {
                        write!(f, "\\+{codepoint:06X}")?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Return a helper which escapes non-ASCII characters using `\XXXX` or
/// `\+XXXXXX` Unicode escape formats (used for `U&'...'` style literals).
pub fn escape_unicode_string(s: &str) -> EscapeUnicodeStringLiteral<'_> {
    EscapeUnicodeStringLiteral(s)
}

/// The side on which `TRIM` should be applied.
///
/// Corresponds to `TRIM(BOTH|LEADING|TRAILING)` SQL syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "visitor", derive(Visit, VisitMut))]
pub enum TrimWhereField {
    /// `BOTH` (trim from both ends)
    Both,
    /// `LEADING` (trim from start)
    Leading,
    /// `TRAILING` (trim from end)
    Trailing,
}

impl fmt::Display for TrimWhereField {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use TrimWhereField::*;
        f.write_str(match self {
            Both => "BOTH",
            Leading => "LEADING",
            Trailing => "TRAILING",
        })
    }
}
