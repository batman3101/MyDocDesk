"""Finds Korean string literals in Rust sources and how each is used."""
import re

HAN = re.compile('[가-힣]')
FMT_MACRO = re.compile(r'(format|println|eprintln|panic)!\(\s*$')
SKIP_BEFORE = re.compile(r'(==|!=|starts_with\(|ends_with\(|contains\(|strip_prefix\(|strip_suffix\(|find\(|split\(|replace\(|split_once\(|rsplit\()\s*$')


def tokens(src):
    """Yields (start, end) of every normal string literal (with quotes), skipping comments,
    char literals and raw strings."""
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith('//', i):
            j = src.find('\n', i)
            i = n if j < 0 else j
            continue
        if src.startswith('/*', i):
            j = src.find('*/', i)
            i = n if j < 0 else j + 2
            continue
        if c == 'r' and (src.startswith('r"', i) or src.startswith('r#', i)) and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == '_')):
            m = re.match(r'r(#*)"', src[i:])
            if m:
                hashes = m.group(1)
                end = src.find('"' + hashes, i + len(m.group(0)))
                i = n if end < 0 else end + 1 + len(hashes)
                continue
        if c == 'b' and src.startswith('b"', i):
            j = i + 2
            while j < n and src[j] != '"':
                j += 2 if src[j] == '\\' else 1
            i = j + 1
            continue
        if c == "'":
            # char literal like 'a', '\n', '\u{..}', '"' ; otherwise a lifetime
            m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\.|[^\\'])'", src[i:])
            if m:
                i += len(m.group(0))
                continue
            i += 1
            continue
        if c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == '\\' else 1
            yield i, j + 1
            i = j + 1
            continue
        i += 1


def test_regions(src):
    regs = []
    for m in re.finditer(r'#\[cfg\(test\)\]\s*\n\s*mod\s+\w+\s*\{', src):
        depth, j = 1, m.end()
        while j < len(src) and depth:
            if src[j] == '{':
                depth += 1
            elif src[j] == '}':
                depth -= 1
            j += 1
        regs.append((m.start(), j))
    return regs


def classify(src, s, e):
    """'fmt' (template of format!), 'plain', or a reason to skip."""
    before = src[max(0, s - 60):s]
    after = src[e:e + 12]
    line_start = src.rfind('\n', 0, s) + 1
    line = src[line_start:src.find('\n', e) if src.find('\n', e) >= 0 else len(src)]
    if re.match(r'\s*(pub(\(\w+\))?\s+)?(const|static)\s', line):
        return 'skip:const'
    if FMT_MACRO.search(before):
        return 'fmt'
    if SKIP_BEFORE.search(before):
        return 'skip:compare'
    if re.match(r'\s*(=>|\|[^|])', after) or re.search(r'"\s*\|\s*$', before):
        return 'skip:pattern'
    return 'plain'


def literals(path):
    src = open(path, encoding='utf-8').read()
    regs = test_regions(src)
    out = []
    for s, e in tokens(src):
        if any(a <= s < b for a, b in regs):
            continue
        body = src[s + 1:e - 1]
        if not HAN.search(body):
            continue
        out.append((s, e, body, classify(src, s, e)))
    return src, out
