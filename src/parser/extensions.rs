//! ALTER EXTENSION syntax shares existing type and routine signature parsers.
use super::*;
impl Parser<'_> {
    pub(super) fn parse_alter_extension(&mut self) -> Result<Statement, ParserError> {
        let name = self.parse_identifier()?;
        let operation = if self.parse_keyword(Keyword::UPDATE) {
            let version = if self.parse_keyword(Keyword::TO) {
                Some(self.parse_identifier()?)
            } else {
                None
            };
            if self.peek_keyword(Keyword::TO) {
                return self.expected_ref("one target version", self.peek_token_ref());
            }
            AlterExtensionOperation::Update { version }
        } else if self.parse_keywords(&[Keyword::SET, Keyword::SCHEMA]) {
            AlterExtensionOperation::SetSchema(self.parse_identifier()?)
        } else {
            let add = match self.expect_one_of_keywords(&[Keyword::ADD, Keyword::DROP])? {
                Keyword::ADD => true,
                _ => false,
            };
            AlterExtensionOperation::Member {
                add,
                object: self.parse_extension_member()?,
            }
        };
        Ok(Statement::AlterExtension(AlterExtension {
            name,
            operation,
        }))
    }
    fn parse_extension_member(&mut self) -> Result<ExtensionMember, ParserError> {
        use ExtensionMemberKind as K;
        let key = self.next_token();
        let Token::Word(word) = &key.token else {
            return self.expected("extension object type", key);
        };
        let named = match word.keyword {
            Keyword::FUNCTION | Keyword::PROCEDURE | Keyword::ROUTINE | Keyword::AGGREGATE => {
                let kind = match word.keyword {
                    Keyword::FUNCTION => ExtensionRoutineKind::Function,
                    Keyword::PROCEDURE => ExtensionRoutineKind::Procedure,
                    Keyword::AGGREGATE => ExtensionRoutineKind::Aggregate,
                    _ => ExtensionRoutineKind::Routine,
                };
                let routine = if kind == ExtensionRoutineKind::Aggregate {
                    {
                        let (mut routine, _, ordered) = self.parse_alter_aggregate_signature()?;
                        if let Some(ordered) = ordered {
                            let args = routine.args.get_or_insert_with(Vec::new);
                            if args
                                .last()
                                .is_some_and(|arg| arg.mode == Some(ArgMode::Variadic))
                                && ordered
                                    .first()
                                    .is_some_and(|arg| arg.mode == Some(ArgMode::Variadic))
                            {
                                args.pop();
                            }
                            args.extend(ordered);
                        }
                        routine
                    }
                } else {
                    self.parse_function_desc()?
                };
                return Ok(ExtensionMember::Routine { kind, routine });
            }
            Keyword::TYPE | Keyword::DOMAIN => {
                return Ok(ExtensionMember::Type {
                    domain: word.keyword == Keyword::DOMAIN,
                    data_type: self.parse_data_type()?,
                });
            }
            Keyword::CAST => {
                self.expect_token(&Token::LParen)?;
                let source = self.parse_data_type()?;
                self.expect_keyword(Keyword::AS)?;
                let target = self.parse_data_type()?;
                self.expect_token(&Token::RParen)?;
                return Ok(ExtensionMember::Cast { source, target });
            }
            Keyword::OPERATOR => {
                let family = if self.parse_keyword(Keyword::FAMILY) {
                    Some(true)
                } else if self.parse_keyword(Keyword::CLASS) {
                    Some(false)
                } else {
                    None
                };
                if let Some(family) = family {
                    let name = self.parse_object_name(false)?;
                    self.expect_keyword(Keyword::USING)?;
                    return Ok(ExtensionMember::OperatorClass {
                        family,
                        name,
                        method: self.parse_identifier()?,
                    });
                }
                let name = self.parse_operator_name()?;
                self.expect_token(&Token::LParen)?;
                let left = if self.parse_keyword(Keyword::NONE) {
                    None
                } else {
                    Some(self.parse_data_type()?)
                };
                self.expect_token(&Token::Comma)?;
                let right = if self.parse_keyword(Keyword::NONE) {
                    None
                } else {
                    Some(self.parse_data_type()?)
                };
                self.expect_token(&Token::RParen)?;
                return Ok(ExtensionMember::Operator { name, left, right });
            }
            Keyword::TRANSFORM => {
                self.expect_keyword(Keyword::FOR)?;
                let data_type = self.parse_data_type()?;
                self.expect_keyword(Keyword::LANGUAGE)?;
                return Ok(ExtensionMember::Transform {
                    data_type,
                    language: self.parse_identifier()?,
                });
            }
            Keyword::ACCESS => {
                self.expect_keyword(Keyword::METHOD)?;
                K::AccessMethod
            }
            Keyword::EVENT => {
                self.expect_keyword(Keyword::TRIGGER)?;
                K::EventTrigger
            }
            Keyword::FOREIGN => {
                if self.parse_keyword(Keyword::TABLE) {
                    K::ForeignTable
                } else {
                    self.expect_keywords(&[Keyword::DATA, Keyword::WRAPPER])?;
                    K::ForeignDataWrapper
                }
            }
            Keyword::MATERIALIZED => {
                self.expect_keyword(Keyword::VIEW)?;
                K::MaterializedView
            }
            Keyword::TEXT => {
                self.expect_keyword(Keyword::SEARCH)?;
                match self.expect_one_of_keywords(&[
                    Keyword::CONFIGURATION,
                    Keyword::DICTIONARY,
                    Keyword::PARSER,
                    Keyword::TEMPLATE,
                ])? {
                    Keyword::CONFIGURATION => K::TextSearchConfiguration,
                    Keyword::DICTIONARY => K::TextSearchDictionary,
                    Keyword::PARSER => K::TextSearchParser,
                    _ => K::TextSearchTemplate,
                }
            }
            Keyword::COLLATION => K::Collation,
            Keyword::CONVERSION => K::Conversion,
            Keyword::LANGUAGE => K::Language,
            Keyword::PROCEDURAL => {
                self.expect_keyword(Keyword::LANGUAGE)?;
                K::Language
            }
            Keyword::SCHEMA => K::Schema,
            Keyword::SEQUENCE => K::Sequence,
            Keyword::SERVER => K::Server,
            Keyword::TABLE => K::Table,
            Keyword::VIEW => K::View,
            Keyword::DATABASE => K::Database,
            Keyword::EXTENSION => K::Extension,
            Keyword::INDEX => K::Index,
            Keyword::PUBLICATION => K::Publication,
            Keyword::ROLE => K::Role,
            Keyword::STATISTICS => K::Statistics,
            Keyword::SUBSCRIPTION => K::Subscription,
            Keyword::TABLESPACE => K::Tablespace,
            _ => return self.expected("extension object type", key),
        };
        Ok(ExtensionMember::Named {
            kind: named,
            name: self.parse_object_name(false)?,
        })
    }
}
