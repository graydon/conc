use std::io::{self, Chars, BufReader, Read};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    from: (usize, usize, usize),
    to: (usize, usize, usize)
}

impl Position {
    fn point(abs: usize, line: usize, column: usize) -> Self {
        Position {
            from: (abs, line, column),
            to: (abs + 1, line, column + 1),
        }
    }

    pub fn cover(self, other: Position) -> Position {
        let from =
            if self.from.0 <= other.from.0 { self.from }
            else { other.from };
        let to =
            if self.to.0 <= other.to.0 { other.to }
            else { self.to };
        
        Position {
            from, to
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lexeme {
    Lparren,
    Rparren,
    Lbracket,
    Tbracket,
    Rbracket,
    Lbrace,
    Rbrace,

    Keyword(String),
    Word(String),
    Mword(String),
    Underscore,
    Comma,
    Dot,
    Rarrow,
    Colon,
    Bar,
    Equals,

    Operator(String),

    String(String),
    Char(char),
    Integer(String),
    Float(String),

    Comment(String),
    DocComment(String),
    TopDocComment(String),

    Indent(usize),
    Unindent(usize),
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub lexeme: Lexeme,
    pub position: Position,
}

#[derive(Debug)]
pub enum LexerError {
    NotUtf8,
    IoError(io::Error),
    UnexpectedEof(Position),
    InvalidInteger(Position),
    MysteriousChar(Position),
}

impl ::std::convert::From<io::CharsError> for LexerError {
    fn from(err: io::CharsError) -> Self {
        use self::io::CharsError::*;
        match err {
            NotUtf8 => LexerError::NotUtf8,
            Other(e) => LexerError::IoError(e),
        }
    }
}

fn is_keyword(kw: &str) -> bool {
    match kw {
        "data"
        | "forall"
        | "case"
        | "where"
        | "if"
        | "else"
        | "infix"
        | "infixl"
        | "infixr" => true,
        _ => false,
    }
}

fn is_whitespace(w: char) -> bool {
    match w {
        ' ' | '\t' => true,
        _ => false,
    }
}

fn is_digit(d: char) -> bool {
    match d {
        '0' ... '9' => true,
        _ => false,
    }
}

fn is_hex_digit(d: char) -> bool {
    match d {
        'a' ... 'f'
        | 'A' ... 'F' => true,
        d => is_digit(d),
    }
}

fn is_letter(l: char) -> bool {
    match l {
        'a' ... 'z'
        | 'A' ... 'Z'
        | '_' => true,
        _ => false,
    }
}

fn is_special(s: char) -> bool {
    match s {
        '!' ... '&'
        | '*' ... '/'
        | ':' ... '@'
        | '\\' | '^'
        | '_' | '|'
        | '~' => true,
        _ => false,
    }
}

#[derive(Debug)]
pub struct Lexer<R: Read> {
    char_iter: Chars<BufReader<R>>,
    cursor: (Option<char>, Option<char>),
    indent: usize,
    seek: usize,
    line: usize,
    column: usize,
}

impl<R: Read> Lexer<R> {
    pub fn new(reader: R) -> Result<Self, LexerError> {
        let mut char_iter = BufReader::new(reader).chars();

        let c = char_iter.next().map_or(Ok(None), |v| v.map(Some))?;
        let la = char_iter.next().map_or(Ok(None), |v| v.map(Some))?;

        Ok(Lexer {
            char_iter,
            cursor: (c, la),
            indent: 0,
            seek: 0,
            line: 0,
            column: 0,
        })
    }

    fn shift(&mut self) -> Result<(), LexerError> {
        let (ch, la) = self.cursor;
        let next = match self.char_iter.next() {
            Some(Ok(c)) =>
                Some(c),
            Some(Err(e)) =>
                return Err(e.into()),
            None => {
                None
            },
        };

        match ch {
            Some('\n') => {
                self.column = 0;
                self.line += 1;
            },
            _ => {
                self.column += 1;
            }
        }

        self.seek += 1;
        self.cursor = (la, next);
        Ok(())
    }

    fn upper_bound(&self) -> (usize, usize, usize) {
        (self.seek + 1, self.line, self.column + 1)
    }

    fn cursor_point(&self) -> (usize, usize, usize) {
        (self.seek, self.line, self.column)
    }

    fn cursor_position(&self) -> Position {
        Position::point(
            self.seek, self.line, self.column
        )
    }

    fn point_token(&self, lex: Lexeme) -> Token {
        Token {
            lexeme: lex,
            position: self.cursor_position(),
        }
    }

    fn skip_whitespace(&mut self) -> Result<Option<Token>, LexerError> {
        let is_line_start = self.column == 0;
        let start_pos = self.cursor_point();

        let mut indent = 0;
        loop {
            let c = self.cursor;
            match c.0 {
                Some(w) if is_whitespace(w) => {
                    indent += 1
                },
                _ => break
            }
            self.shift()?
        }

        if is_line_start && self.cursor.0 != Some('\n') {
            let position = Position {
                from: start_pos,
                to: self.cursor_point(),
            };

            use std::cmp::Ordering::*;
            let res = match indent.cmp(&self.indent) {
                Greater => Some(Token {
                    position,
                    lexeme: Lexeme::Indent(indent - self.indent),
                }),
                Less => Some(Token {
                    position,
                    lexeme: Lexeme::Unindent(self.indent - indent),
                }),
                Equal => None,

            };

            self.indent = indent;

            Ok(res)
        } else {
            Ok(None)
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexerError> {
        if let Some(tok) = self.skip_whitespace()? {
            return Ok(tok)
        }

        let (c, la) = self.cursor;
        let c = match c {
            Some(c) => c,
            None => return Ok(self.point_token(Lexeme::Eof)),
        };

        use self::Lexeme::*;
        let tok = match c {
            '-' if la == Some('-') => self.comment(),
            '[' => Ok(self.point_token(Lbracket)),
            'T' if la == Some('[') => self.t_bracket(),
            ']' => Ok(self.point_token(Rbracket)),
            '{' => Ok(self.point_token(Lbrace)),
            '}' => Ok(self.point_token(Rbrace)),
            '(' => Ok(self.point_token(Lparren)),
            ')' => Ok(self.point_token(Rparren)),
            '\n' => Ok(self.point_token(Newline)),
            '"' => self.string(),
            '\'' => self.char(),
            '0' if la == Some('x') => self.hex_integer(),
            s if is_special(s) => self.operator(),
            l if is_letter(l) => self.word(),
            d if is_digit(d) => self.integer_or_word(),
            _ => {
                let pos = self.cursor_position();
                return Err(LexerError::MysteriousChar(pos));
            },
        };

        self.shift()?;

        tok
    }

    fn read_word(&mut self) -> Result<String, LexerError> {
        let mut word = String::new();
        loop {
            let eow = !self.cursor.1
                .map(|c| is_letter(c) || is_digit(c))
                .unwrap_or(false);
            word.push(self.cursor.0.unwrap());

            if eow { break }
            self.shift()?;
        }

        Ok(word)
    }

    fn word(&mut self) -> Result<Token, LexerError> {
        let start_pos = self.cursor_point();

        let word = self.read_word()?;
        let is_mword = match self.cursor.1 {
            Some('[') => {
                self.shift()?;
                true
            },
            _ => false
        };
        let lexeme = match &*word {
            "_" => Lexeme::Underscore,
            _ if is_mword => Lexeme::Mword(word),
            _ if is_keyword(&*word) => Lexeme::Keyword(word),
            _ => Lexeme::Word(word),
        };

        let position = Position {
            from: start_pos,
            to: self.upper_bound(),
        };

        Ok(Token {
            position,
            lexeme,
        })
    }

    fn integer_or_word(&mut self) -> Result<Token, LexerError> {
        let start_pos = self.cursor_point();

        let mut thing = String::new();
        let lexeme;
        loop {
            thing.push(self.cursor.0.unwrap());

            match self.cursor.1 {
                Some(d) if is_digit(d) => (),
                Some(l) if is_letter(l) => {
                    let rem = self.read_word()?;
                    thing.push_str(&rem);

                    let lexeme = match self.cursor.1 {
                        Some('[') => {
                            self.shift()?;
                            Lexeme::Mword(thing)
                        },
                        _ => Lexeme::Word(thing)
                    };

                    let position = Position {
                        from: start_pos,
                        to: self.upper_bound(),
                    };

                    return Ok(Token {
                        position,
                        lexeme,
                    })
                },
                Some('.') => {
                    thing.push('.');
                    self.shift()?;

                    if !self.cursor.0.map(is_digit).unwrap_or(false) {
                        let pos = Position {
                            from: start_pos,
                            to: self.upper_bound(),
                        };
                        return Err(LexerError::InvalidInteger(pos))
                    }

                    while self.cursor.1.map(is_digit).unwrap_or(false) {
                        thing.push(self.cursor.0.unwrap());
                        self.shift()?;
                    }

                    lexeme = Lexeme::Float(thing);
                    break
                },
                _ => {
                    lexeme = Lexeme::Integer(thing);
                    break
                },
            }

            self.shift()?;
        }

        let position = Position {
            from: start_pos,
            to: self.upper_bound(),
        };

        Ok(Token {
            position,
            lexeme
        })
    }

    fn comment(&mut self) -> Result<Token, LexerError> {
        let start_pos = self.cursor_point();
        self.shift()?;
        self.shift()?;

        let lex = match self.cursor.0 {
            Some('.') => {
                self.shift()?;
                Lexeme::DocComment
            },
            Some('^') => {
                self.shift()?;
                Lexeme::TopDocComment
            },
            _ => Lexeme::Comment,
        };

        let mut line = String::new();
        loop {
            match self.cursor.0 {
                Some('\n') | None => break,
                Some(c) => line.push(c),
            }
            self.shift()?;
        }

        let position = Position {
            from: start_pos,
            to: self.upper_bound(),
        };

        Ok(Token {
            position,
            lexeme: lex(line),
        })
    }

    fn string(&mut self) -> Result<Token, LexerError> {
        let start_pos = self.cursor_point();
        self.shift()?;

        let mut string = String::new();
        loop {
            match self.cursor {
                (Some('\\'), Some('"')) => {
                    string.push('"');
                    self.shift()?;
                },
                (Some('\\'), Some('\\')) => {
                    string.push('\\');
                    self.shift()?;
                },
                (Some('"'), _) => break,
                (Some(c), _) => string.push(c),
                (None, _) => {
                    let pos = self.cursor_position();
                    return Err(LexerError::UnexpectedEof(pos))
                },
            }

            self.shift()?;
        }

        let position = Position {
            from: start_pos,
            to: self.upper_bound(),
        };

        Ok(Token {
            position,
            lexeme: Lexeme::String(string)
        })
    }

    fn char(&mut self) -> Result<Token, LexerError> {
        unimplemented!()
    }

    fn hex_integer(&mut self) -> Result<Token, LexerError> {
        let start_pos = self.cursor_point();
        self.shift()?;
        self.shift()?;

        let mut number = "0x".to_string();
        self.shift()?;
        loop {
            let eon = self.cursor.1
                .map(|w| is_whitespace(w) || w == '\n')
                .unwrap_or(true);

            match self.cursor.0 {
                Some(d) if is_hex_digit(d) =>{
                    number.push(d)
                },
                _ => unimplemented!()
            }

            if eon { break }
            self.shift()?;
        }

        let position = Position {
            from: start_pos,
            to: self.upper_bound(),
        };

        Ok(Token {
            position,
            lexeme: Lexeme::Integer(number),
        })
    }

    fn t_bracket(&mut self) -> Result<Token, LexerError> {
        let position = Position {
            from: (self.seek, self.line, self.column),
            to: (self.seek + 1, self.line, self.column + 1)
        };
        self.shift()?;

        Ok(Token {
            position,
            lexeme: Lexeme::Tbracket,
        })
    }

    fn operator(&mut self) -> Result<Token, LexerError> {
        let start_pos = self.cursor_point();

        let mut operator = String::new();
        loop {
            let eop = !self.cursor.1.map(is_special).unwrap_or(false);
            operator.push(self.cursor.0.unwrap());

            if eop { break }
            self.shift()?;
        }

        let position = Position {
            from: start_pos,
            to: self.upper_bound()
        };

        let lexeme = match &*operator {
            "," => Lexeme::Comma,
            "." => Lexeme::Dot,
            "->" => Lexeme::Rarrow,
            ":" => Lexeme::Colon,
            "|" => Lexeme::Bar,
            "=" => Lexeme::Equals,
            _ => Lexeme::Operator(operator)
        };

        Ok(Token {
            position,
            lexeme,
        })
    }
}

//#[cfg(test)]
mod tests {
    use super::Lexeme;
    use super::{Lexer, LexerError};

    fn collect_lexemes<R: ::std::io::Read>(lexer: &mut Lexer<R>) -> Vec<super::Lexeme> {
        let mut lexemes = vec![];
        loop {
            let tok = lexer.next_token().unwrap();

            if tok.lexeme == Lexeme::Eof {
                lexemes.push(Lexeme::Eof);
                break
            }

            lexemes.push(tok.lexeme)
        }
        lexemes
    }

    fn collect_positions<R: ::std::io::Read>(lexer: &mut Lexer<R>) -> Vec<super::Position> {
        let mut positions = vec![];
        loop {
            let tok = lexer.next_token().unwrap();

            if tok.lexeme == Lexeme::Eof {
                break
            }

            positions.push(tok.position)
        }
        positions
    }

    fn draw_positions(positions: &[super::Position]) -> String {
        let chars = ['x', 'y'];

        let mut line = 0;
        let mut column = 0;

        let mut string = String::new();

        for (i, p) in positions.iter().enumerate() {
            while line < p.from.1 {
                string.push('\n');
                line += 1;
                column = 0;
            }

            while column < p.from.2 {
                string.push(' ');
                column += 1;
            }

            let delta = p.to.2 - p.from.2;
            for _ in 0..delta {
                string.push(chars[i % 2]);
                column += 1;
            }
        }
    
        string
    }

    #[test] fn hello_lexer() {
        use self::Lexeme::*;
        let hello = concat!(
            "main =\n",
            "    \"Hello, world!\" print_ln\n"
        ).as_bytes();

        let mut lexer = Lexer::new(hello).unwrap();
        let mut lexemes = collect_lexemes(&mut lexer);

        assert_eq!(&*lexemes, &[
            Word("main".to_string()), Equals, Newline,
            Indent(4), String("Hello, world!".to_string()), Word("print_ln".to_string()), Newline,
            Unindent(4), Eof
        ])
    }

    #[test] fn hello_positions() {
        use self::Lexeme::*;
        let hello = concat!(
            "main =\n",
            "    \"Hello, world!\" print_ln\n",
            "    \"Hello, world!\" print_ln\n",
            "    \"Hello, world!\" print_ln"
        ).as_bytes();

        let mut lexer = Lexer::new(hello).unwrap();
        let mut positions = collect_positions(&mut lexer);

        let pretty = draw_positions(&positions);
        let gold =
r#"xxxx yx
yyyyxxxxxxxxxxxxxxx yyyyyyyyx
    yyyyyyyyyyyyyyy xxxxxxxxy
    xxxxxxxxxxxxxxx yyyyyyyy"#;

        assert_eq!(gold, &*pretty)
    }

    #[test] fn stairs() {
        use self::Lexeme::*;
        let faboor = concat!(
            "if foo:\n",
            "    if bar:\n",
            "        bar foo\n",
            "\n",
            "    foo bar\n",
            "else: \n",
            "    baz baz baz\n"
        ).as_bytes();

        let mut lexer = Lexer::new(faboor).unwrap();
        let mut lexemes = collect_lexemes(&mut lexer);

        assert_eq!(&*lexemes, &[
            Keyword("if".to_string()), Word("foo".to_string()), Colon, Newline,
            Indent(4), Keyword("if".to_string()), Word("bar".to_string()), Colon, Newline,
            Indent(4), Word("bar".to_string()), Word("foo".to_string()), Newline,
            Newline,
            Unindent(4), Word("foo".to_string()), Word("bar".to_string()), Newline,
            Unindent(4), Keyword("else".to_string()), Colon, Newline,
            Indent(4), Word("baz".to_string()), Word("baz".to_string()), Word("baz".to_string()), Newline,
            Unindent(4), Eof
        ])
    }
}
