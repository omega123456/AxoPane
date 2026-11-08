import { findIfElseLines } from '../../../scripts/check-rust-else.mjs'

function rustSource(lines: string[]) {
  return lines.join('\n')
}

describe('findIfElseLines', () => {
  it('reports the line of every if/else and else-if', () => {
    const source = rustSource([
      'fn pick(flag: bool, other: bool) -> u8 {',
      '    if flag { 1 } else { 2 };',
      '    if flag {',
      '        3',
      '    } else if other {',
      '        4',
      '    } else {',
      '        5',
      '    }',
      '}',
    ])

    expect(findIfElseLines(source)).toEqual([2, 5, 7])
  })

  it('allows let-else on one or several lines', () => {
    const source = rustSource([
      'fn first(values: &[u8]) -> Option<u8> {',
      '    let Some(first) = values.first() else {',
      '        return None;',
      '    };',
      '    let Ok(parsed) = "1".parse::<u8>()',
      '    else {',
      '        return None;',
      '    };',
      '    Some(*first + parsed)',
      '}',
    ])

    expect(findIfElseLines(source)).toEqual([])
  })

  it('ignores else inside line, doc and nested block comments', () => {
    const source = rustSource([
      '// if a { } else { }',
      '/// if a { } else { }',
      '/* outer /* inner } else { */ still comment } else { */',
      'fn plain() {}',
    ])

    expect(findIfElseLines(source)).toEqual([])
  })

  it('ignores else inside string, byte string and raw string literals', () => {
    const source = rustSource([
      'const PLAIN: &str = "} else { \\" } else {";',
      'const BYTES: &[u8] = b"} else {";',
      'const RAW: &str = r#"} else { "# ;',
      'const RAW_BYTES: &[u8] = br"} else {";',
      'const C_TEXT: &core::ffi::CStr = c"} else {";',
    ])

    expect(findIfElseLines(source)).toEqual([])
  })

  it('keeps counting lines through multi-line strings', () => {
    const source = rustSource([
      'const TEXT: &str = "first',
      'second } else {',
      'third";',
      'fn pick(flag: bool) -> u8 { if flag { 1 } else { 2 } }',
    ])

    expect(findIfElseLines(source)).toEqual([4])
  })

  it('treats brace and quote char literals as literals', () => {
    const source = rustSource([
      "const OPEN: char = '{';",
      "const CLOSE: char = '}';",
      "const QUOTE: char = '\\'';",
      "const BYTE_QUOTE: u8 = b'\\'';",
      "const BYTE_BRACE: u8 = b'}';",
      'fn pick(flag: bool) -> u8 { if flag { 1 } else { 2 } }',
    ])

    expect(findIfElseLines(source)).toEqual([6])
  })

  it('reads lifetimes and raw identifiers without losing its place', () => {
    const source = rustSource([
      "fn longest<'a>(left: &'a str, right: &'a str) -> &'a str {",
      '    let r#type = left.len() > right.len();',
      '    if r#type { left } else { right }',
      '}',
    ])

    expect(findIfElseLines(source)).toEqual([3])
  })
})
