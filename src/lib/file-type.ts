import type { DirectoryEntry } from '@/lib/types/ipc'

/**
 * Coarse, presentation-only classification of a directory entry derived purely
 * from its name. There is no IO and no folder inspection: categories drive the
 * per-type glyph + color in {@link import('@/components/icons/EntryIcon')}.
 */
export type FileCategory =
  | 'folder'
  | 'code'
  | 'web'
  | 'config'
  | 'text'
  | 'office'
  | 'image'
  | 'audio'
  | 'video'
  | 'archive'
  | 'disc'
  | 'executable'
  | 'font'
  | 'database'
  | 'generic'

/** Special-folder glyph kinds, resolved by name (never by contents). */
export type FolderGlyphKind = 'downloads' | 'git' | 'modules' | 'open' | 'closed'

const EXTENSIONS: Record<Exclude<FileCategory, 'folder' | 'generic'>, string[]> = {
  image: [
    'jpg', 'jpeg', 'jpe', 'jfif', 'png', 'apng', 'gif', 'bmp', 'dib', 'webp',
    'svg', 'svgz', 'tif', 'tiff', 'ico', 'cur', 'heic', 'heif', 'avif', 'jxl',
    'jp2', 'j2k', 'raw', 'cr2', 'cr3', 'nef', 'arw', 'dng', 'orf', 'rw2', 'raf',
    'sr2', 'srw', 'pef', 'nrw', 'psd', 'xcf', 'ai', 'eps', 'tga', 'pcx', 'ppm',
    'pgm', 'pbm', 'pnm', 'exr', 'hdr', 'qoi',
  ],
  video: [
    'mp4', 'm4v', 'mkv', 'webm', 'avi', 'mov', 'qt', 'wmv', 'flv', 'f4v', 'swf',
    'mpg', 'mpeg', 'mpe', 'm1v', 'm2v', 'mp2', 'm2ts', 'mts', 'ts', 'tsv', 'vob',
    'ogv', 'ogm', '3gp', '3g2', 'rm', 'rmvb', 'asf', 'divx', 'xvid', 'mxf', 'y4m',
  ],
  audio: [
    'mp3', 'flac', 'wav', 'wave', 'aac', 'm4a', 'm4b', 'm4p', 'ogg', 'oga',
    'opus', 'wma', 'aiff', 'aif', 'aifc', 'alac', 'ape', 'wv', 'mid', 'midi',
    'kar', 'amr', 'ac3', 'eac3', 'dts', 'au', 'snd', 'ra', 'weba', 'mka', 'caf',
    'dsf', 'dff',
  ],
  archive: [
    'zip', '7z', 'rar', 'tar', 'gz', 'tgz', 'bz2', 'tbz', 'tbz2', 'xz', 'txz',
    'zst', 'zstd', 'lz', 'lzma', 'lz4', 'lzo', 'lha', 'lzh', 'z', 'cab', 'arj',
    'ace', 'cpio', 'jar', 'war', 'ear', 'gem', 'whl', 'crx', 'xpi', 'pak', 'pk3',
    'vpk',
  ],
  disc: [
    'iso', 'img', 'dmg', 'vhd', 'vhdx', 'vmdk', 'vdi', 'qcow', 'qcow2', 'toast',
    'cue', 'bin', 'nrg', 'mds', 'mdf', 'udf', 'wim',
  ],
  code: [
    'js', 'mjs', 'cjs', 'jsx', 'ts', 'mts', 'cts', 'tsx', 'py', 'pyw', 'pyi',
    'pyx', 'rb', 'rbw', 'go', 'rs', 'c', 'h', 'i', 'cc', 'cpp', 'cxx', 'c++',
    'hpp', 'hh', 'hxx', 'm', 'mm', 'cs', 'java', 'kt', 'kts', 'scala', 'sc',
    'swift', 'php', 'php5', 'phtml', 'pl', 'pm', 'pod', 'lua', 'r', 'jl', 'dart',
    'clj', 'cljs', 'cljc', 'edn', 'ex', 'exs', 'erl', 'hrl', 'hs', 'lhs', 'ml',
    'mli', 'fs', 'fsi', 'fsx', 'vb', 'vbs', 'asm', 's', 'nasm', 'nim', 'zig',
    'v', 'sv', 'sol', 'groovy', 'gradle', 'd', 'di', 'pas', 'pp', 'f', 'f77',
    'f90', 'f95', 'for', 'coffee', 'elm', 'purs', 'rkt', 'scm', 'ss', 'lisp',
    'el', 'tcl', 'awk', 'sh', 'bash', 'zsh', 'fish', 'ps1', 'psm1', 'psd1',
    'bat', 'cmd', 'wasm', 'gd',
  ],
  web: [
    'html', 'htm', 'xhtml', 'shtml', 'css', 'scss', 'sass', 'less', 'styl',
    'pcss', 'vue', 'svelte', 'astro', 'hbs', 'handlebars', 'ejs', 'pug', 'jade',
    'haml', 'slim', 'mustache', 'twig', 'liquid', 'njk',
  ],
  config: [
    'json', 'json5', 'jsonc', 'yaml', 'yml', 'toml', 'ini', 'cfg', 'conf',
    'config', 'env', 'properties', 'xml', 'xsd', 'xsl', 'xslt', 'plist',
    'editorconfig', 'lock', 'gitignore', 'gitattributes', 'dockerignore',
    'dockerfile', 'csv', 'tsv', 'psv', 'ndjson', 'har', 'proto', 'graphql', 'gql',
  ],
  text: [
    'txt', 'text', 'md', 'markdown', 'mdx', 'rst', 'adoc', 'asciidoc', 'nfo',
    'log', 'readme', 'me', 'org', 'wiki', 'tex', 'bib', 'ltx', 'srt', 'vtt',
    'sub', 'ass',
  ],
  office: [
    'pdf', 'doc', 'docx', 'dot', 'dotx', 'docm', 'odt', 'fodt', 'rtf', 'xls',
    'xlsx', 'xlsm', 'xlsb', 'xlt', 'xltx', 'xltm', 'ods', 'fods', 'ppt', 'pptx',
    'pps', 'ppsx', 'pptm', 'ppsm', 'odp', 'fodp', 'pages', 'numbers', 'key',
    'epub', 'mobi', 'azw', 'azw3', 'kfx', 'fb2', 'djvu', 'djv', 'xps', 'oxps',
  ],
  font: [
    'ttf', 'otf', 'ttc', 'otc', 'woff', 'woff2', 'eot', 'pfb', 'pfa', 'pfm',
    'afm', 'fon', 'fnt', 'bdf', 'pcf',
  ],
  database: [
    'db', 'sqlite', 'sqlite3', 'sql', 'mdb', 'accdb', 'dbf', 'db3', 's3db',
    'sdb', 'myd', 'frm', 'parquet', 'avro', 'orc', 'arrow', 'feather', 'realm',
  ],
  executable: [
    'exe', 'msi', 'msix', 'appx', 'app', 'pkg', 'deb', 'rpm', 'apk', 'aab',
    'appimage', 'flatpak', 'snap', 'run', 'com', 'scr', 'sys', 'dll', 'so',
    'dylib', 'o', 'obj', 'a', 'lib', 'ko', 'efi',
  ],
}

/**
 * Extensionless names that nonetheless carry a well-known category. Matched
 * case-insensitively against the whole name (e.g. `Dockerfile`, `.gitignore`).
 */
const SPECIAL_NAMES = new Map<string, FileCategory>([
  ['.gitignore', 'config'],
  ['.gitattributes', 'config'],
  ['.dockerignore', 'config'],
  ['.editorconfig', 'config'],
  ['dockerfile', 'config'],
])

const EXTENSION_CATEGORY = new Map<string, FileCategory>()
for (const [category, extensions] of Object.entries(EXTENSIONS) as [FileCategory, string[]][]) {
  for (const ext of extensions) {
    EXTENSION_CATEGORY.set(ext, category)
  }
}

/**
 * Classify an entry by name alone. Folders short-circuit to `folder`; files are
 * matched on their last `.`-segment (so `archive.tar.gz` → `gz` → archive).
 * Extensionless and dotfile names fall back to a small known set, else generic.
 */
export function getFileCategory(entry: Pick<DirectoryEntry, 'name' | 'isDir'>): FileCategory {
  if (entry.isDir) {
    return 'folder'
  }

  const name = entry.name.toLowerCase()

  const special = SPECIAL_NAMES.get(name)
  if (special) {
    return special
  }

  const dot = name.lastIndexOf('.')
  // No dot, or a leading dot with nothing after the basename (e.g. `.env` has
  // its category covered by SPECIAL_NAMES; a bare `.foo` is treated as generic).
  if (dot <= 0) {
    return 'generic'
  }

  const ext = name.slice(dot + 1)
  return EXTENSION_CATEGORY.get(ext) ?? 'generic'
}

/**
 * Resolve the folder glyph kind from a folder name (case-insensitive exact
 * match) and its expansion state. Kept deliberately small and OS-agnostic.
 */
export function getFolderGlyphKind(name: string, isOpen: boolean): FolderGlyphKind {
  switch (name.toLowerCase()) {
    case 'downloads':
      return 'downloads'
    case '.git':
      return 'git'
    case 'node_modules':
      return 'modules'
    default:
      return isOpen ? 'open' : 'closed'
  }
}
