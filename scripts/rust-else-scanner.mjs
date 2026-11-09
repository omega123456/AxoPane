const ELSE_KEYWORD = 'else'
const BLOCK_CLOSE = '}'
const NEWLINE = '\n'
const BACKSLASH = '\\'
const DOUBLE_QUOTE = '"'
const SINGLE_QUOTE = "'"
const HASH = '#'
const LINE_COMMENT_OPENER = '//'
const BLOCK_COMMENT_OPENER = '/*'
const BLOCK_COMMENT_CLOSER = '*/'
const RAW_STRING_PREFIXES = new Set(['r', 'br', 'cr'])
const QUOTED_STRING_PREFIXES = new Set(['b', 'c'])
const BYTE_CHAR_PREFIX = 'b'
const IDENTIFIER_START = /[A-Za-z_]/
const IDENTIFIER_PART = /[A-Za-z0-9_]/
const WHITESPACE = /\s/
const CHAR_LITERAL_CLOSE_OFFSET = 2

class RustSourceScanner {
  constructor(source) {
    this.characters = Array.from(source)
    this.position = 0
    this.line = 1
    this.tokens = []
  }

  scanTokens() {
    while (this.hasMoreCharacters()) {
      this.scanNextToken()
    }
    return this.tokens
  }

  hasMoreCharacters() {
    return this.position < this.characters.length
  }

  peek(offset = 0) {
    return this.characters[this.position + offset] ?? ''
  }

  peekPair() {
    return this.peek() + this.peek(1)
  }

  advance() {
    const character = this.peek()
    this.position += 1
    if (character === NEWLINE) {
      this.line += 1
    }
    return character
  }

  scanNextToken() {
    const character = this.peek()
    if (WHITESPACE.test(character)) {
      this.advance()
      return
    }
    if (this.peekPair() === LINE_COMMENT_OPENER) {
      this.skipLineComment()
      return
    }
    if (this.peekPair() === BLOCK_COMMENT_OPENER) {
      this.skipBlockComment()
      return
    }
    if (character === DOUBLE_QUOTE) {
      this.skipQuotedString()
      return
    }
    if (character === SINGLE_QUOTE) {
      this.skipCharLiteralOrLifetime()
      return
    }
    if (IDENTIFIER_START.test(character)) {
      this.scanIdentifierOrPrefixedLiteral()
      return
    }
    this.pushToken(this.advance())
  }

  pushToken(text) {
    this.tokens.push({ text, line: this.line })
  }

  skipLineComment() {
    while (this.hasMoreCharacters() && this.peek() !== NEWLINE) {
      this.advance()
    }
  }

  skipBlockComment() {
    let openCommentCount = 0
    do {
      openCommentCount += this.skipCommentDelimiter()
    } while (this.hasMoreCharacters() && openCommentCount > 0)
  }

  skipCommentDelimiter() {
    const pair = this.peekPair()
    if (pair === BLOCK_COMMENT_OPENER) {
      this.advance()
      this.advance()
      return 1
    }
    if (pair === BLOCK_COMMENT_CLOSER) {
      this.advance()
      this.advance()
      return -1
    }
    this.advance()
    return 0
  }

  skipQuotedString() {
    this.advance()
    while (this.hasMoreCharacters() && this.peek() !== DOUBLE_QUOTE) {
      this.skipCharacterOrEscape()
    }
    this.advance()
  }

  skipCharacterOrEscape() {
    if (this.peek() === BACKSLASH) {
      this.advance()
    }
    this.advance()
  }

  skipCharLiteralOrLifetime() {
    if (!this.isCharLiteralAhead()) {
      this.advance()
      return
    }
    this.advance()
    this.skipCharacterOrEscape()
    while (this.hasMoreCharacters() && this.peek() !== SINGLE_QUOTE) {
      this.advance()
    }
    this.advance()
  }

  isCharLiteralAhead() {
    const startsWithEscape = this.peek(1) === BACKSLASH
    const closesAfterOneCharacter = this.peek(CHAR_LITERAL_CLOSE_OFFSET) === SINGLE_QUOTE
    return startsWithEscape || closesAfterOneCharacter
  }

  scanIdentifierOrPrefixedLiteral() {
    const identifier = this.readIdentifier()
    if (this.isRawStringAhead(identifier)) {
      this.skipRawString()
      return
    }
    if (this.isPrefixedQuotedStringAhead(identifier)) {
      this.skipQuotedString()
      return
    }
    if (this.isByteCharAhead(identifier)) {
      this.skipCharLiteralOrLifetime()
      return
    }
    this.pushToken(identifier)
  }

  readIdentifier() {
    let identifier = ''
    while (IDENTIFIER_PART.test(this.peek())) {
      identifier += this.advance()
    }
    return identifier
  }

  isRawStringAhead(identifier) {
    if (!RAW_STRING_PREFIXES.has(identifier)) {
      return false
    }
    let offset = 0
    while (this.peek(offset) === HASH) {
      offset += 1
    }
    return this.peek(offset) === DOUBLE_QUOTE
  }

  isPrefixedQuotedStringAhead(identifier) {
    return QUOTED_STRING_PREFIXES.has(identifier) && this.peek() === DOUBLE_QUOTE
  }

  isByteCharAhead(identifier) {
    return identifier === BYTE_CHAR_PREFIX && this.peek() === SINGLE_QUOTE
  }

  skipRawString() {
    let hashCount = 0
    while (this.peek() === HASH) {
      this.advance()
      hashCount += 1
    }
    this.advance()
    const terminator = DOUBLE_QUOTE + HASH.repeat(hashCount)
    while (this.hasMoreCharacters() && !this.isAtText(terminator)) {
      this.advance()
    }
    this.skipText(terminator)
  }

  skipText(text) {
    this.position += Array.from(text).length
  }

  isAtText(text) {
    return Array.from(text).every((character, offset) => this.peek(offset) === character)
  }
}

// Rust rejects a `}` directly before the `else` of a let-else, so only an if/else can follow a block.
function isIfElse(tokens, index) {
  const token = tokens[index]
  const previousToken = tokens[index - 1]
  if (token.text !== ELSE_KEYWORD) {
    return false
  }
  return previousToken?.text === BLOCK_CLOSE
}

export function findIfElseLines(source) {
  const tokens = new RustSourceScanner(source).scanTokens()
  return tokens.filter((_token, index) => isIfElse(tokens, index)).map((token) => token.line)
}
