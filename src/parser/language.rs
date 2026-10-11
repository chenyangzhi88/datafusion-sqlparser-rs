//! PostgreSQL language grammar; parameterless CREATE is an extension alias.
use super::*;
impl Parser<'_> {
    pub(super) fn language_word(&mut self, expected: &str) -> bool {
        if matches!(&self.peek_token_ref().token,Token::Word(w) if w.quote_style.is_none()&&w.value.eq_ignore_ascii_case(expected))
        {
            self.next_token();
            true
        } else {
            false
        }
    }
    pub(super) fn language_prefix(&self) -> bool {
        matches!(&self.peek_token_ref().token,Token::Word(w) if w.quote_style.is_none() && ["LANGUAGE","PROCEDURAL","TRUSTED"].iter().any(|s|w.value.eq_ignore_ascii_case(s)))
    }
    pub(super) fn parse_postgres_language_create(
        &mut self,
        or_replace: bool,
    ) -> Result<Statement, ParserError> {
        let trusted = self.language_word("TRUSTED");
        let _ = self.language_word("PROCEDURAL");
        self.expect_keyword(Keyword::LANGUAGE)?;
        let name = self.parse_identifier()?;
        if !self.language_word("HANDLER") {
            return Ok(Statement::CreateExtension(CreateExtension {
                name,
                if_not_exists: or_replace,
                cascade: false,
                schema: None,
                version: None,
            }));
        }
        let handler = self.parse_object_name(false)?;
        let inline = if self.language_word("INLINE") {
            Some(self.parse_object_name(false)?)
        } else {
            None
        };
        let validator = if self.language_word("VALIDATOR") {
            Some(self.parse_object_name(false)?)
        } else {
            None
        };
        Ok(Statement::CreateLanguage(CreateLanguage {
            or_replace,
            trusted,
            name,
            handler,
            inline,
            validator,
        }))
    }
    pub(super) fn parse_postgres_language_drop(&mut self) -> Result<Statement, ParserError> {
        let _ = self.language_word("PROCEDURAL");
        self.expect_keyword(Keyword::LANGUAGE)?;
        let if_exists = self.parse_keywords(&[Keyword::IF, Keyword::EXISTS]);
        let names = self.parse_comma_separated(Parser::parse_identifier)?;
        let cascade = self.parse_keyword(Keyword::CASCADE);
        if !cascade {
            let _ = self.parse_keyword(Keyword::RESTRICT);
        }
        Ok(Statement::DropLanguage(DropLanguage {
            if_exists,
            names,
            cascade,
        }))
    }
    pub(super) fn parse_postgres_language_alter(&mut self) -> Result<Statement, ParserError> {
        let _ = self.language_word("PROCEDURAL");
        self.expect_keyword(Keyword::LANGUAGE)?;
        let name = self.parse_identifier()?;
        self.expect_keywords(&[Keyword::RENAME, Keyword::TO])?;
        let new_name = self.parse_identifier()?;
        Ok(Statement::AlterLanguage(AlterLanguage { name, new_name }))
    }
}
