use tokenizers::Tokenizer;

/// Streaming token decoder — handles partial UTF-8 sequences and
/// outputs clean text fragments as tokens are generated.
pub struct TokenOutputStream {
    tokenizer: Tokenizer,
    tokens: Vec<u32>,
    prev_index: usize,
    current_index: usize,
}

impl TokenOutputStream {
    pub fn new(tokenizer: Tokenizer) -> Self {
        Self {
            tokenizer,
            tokens: Vec::new(),
            prev_index: 0,
            current_index: 0,
        }
    }

    /// Feed the next generated token. Returns decoded text if a complete
    /// character boundary was reached, or None if we're mid-character.
    pub fn next_token(&mut self, token: u32) -> anyhow::Result<Option<String>> {
        let prev_text = if self.tokens.is_empty() {
            String::new()
        } else {
            let tokens = &self.tokens[self.prev_index..self.current_index];
            self.tokenizer
                .decode(tokens, true)
                .map_err(anyhow::Error::msg)?
        };

        self.tokens.push(token);
        self.current_index = self.tokens.len();

        let text = self
            .tokenizer
            .decode(&self.tokens[self.prev_index..self.current_index], true)
            .map_err(anyhow::Error::msg)?;

        if text.len() > prev_text.len()
            && text.ends_with(|c: char| {
                !c.is_ascii()
                    || c.is_ascii_alphanumeric()
                    || c.is_ascii_punctuation()
                    || c == ' '
                    || c == '\n'
            })
        {
            let new_text = text[prev_text.len()..].to_string();
            self.prev_index = self.current_index - 1; // keep last token for context
            Ok(Some(new_text))
        } else if text.len() > prev_text.len() {
            let new_text = text[prev_text.len()..].to_string();
            Ok(Some(new_text))
        } else {
            Ok(None)
        }
    }

    /// Flush any remaining buffered text.
    pub fn flush(&self) -> anyhow::Result<Option<String>> {
        if self.tokens.is_empty() {
            return Ok(None);
        }
        let text = self
            .tokenizer
            .decode(&self.tokens[self.prev_index..], true)
            .map_err(anyhow::Error::msg)?;
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }
}
