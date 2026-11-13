# ALREADY APPLIED (2026-11-13). Do not run again: it would wrap translated text twice.
# To add a phrase now: write crate::i18n::tr("한국어") or crate::ft!("한국어 {}", "Tiếng Việt {}", x)
# in the code, and add the "한국어" => "Tiếng Việt" line to the table in src/i18n.rs (and vi.txt).
import re, sys
sys.path.insert(0, 'i18n_tools')
from scan import literals

FILES = ['src/app.rs', 'src/ai_page.rs', 'src/analyze_page.rs', 'src/pipeline_page.rs', 'src/eval_ai.rs', 'src/ai.rs', 'src/cli.rs', 'src/sync.rs', 'src/local.rs']
SKIP = {
    '\\n\\n## 그림 {} (AI 해석)\\n{}',  # marker read back from document text
    '[지시]\\n{system}\\n\\n{user}', 'You are a concise assistant for a personal document app. Follow the [지시] block in the user message.', '위 입력의 [지시]를 따르세요.',
    'G:\\\\내 드라이브', 'H:\\\\내 드라이브', '내 PC', '공유 문서',
}

vi = {}
for line in open('i18n_tools/vi.txt', encoding='utf-8'):
    line = line.rstrip('\n')
    if ' ||| ' not in line:
        continue
    k, v = line.split(' ||| ', 1)
    vi[k] = v

PH = re.compile(r'(?<!\{)\{([^{}]*)\}(?!\})')


def placeholders(t):
    implicit, explicit, named = 0, set(), set()
    for m in PH.finditer(t.replace('{{', '').replace('}}', '')):
        inner = m.group(1).split(':', 1)[0]
        if inner == '':
            implicit += 1
        elif inner.isdigit():
            explicit.add(int(inner))
        else:
            named.add(inner)
    npos = max([implicit] + [i + 1 for i in explicit])
    return npos, named


def matching_paren(src, i):
    depth = 0
    j = i
    in_str = False
    while j < len(src):
        c = src[j]
        if in_str:
            if c == '\\':
                j += 2
                continue
            if c == '"':
                in_str = False
        elif c == '"':
            in_str = True
        elif c == '(':
            depth += 1
        elif c == ')':
            depth -= 1
            if depth == 0:
                return j
        j += 1
    raise ValueError('no closing paren')


errors = []
used = set()
for f in FILES:
    src, lits = literals(f)
    for s, e, body, kind in sorted(lits, key=lambda x: -x[0]):
        if kind.startswith('skip') or body in SKIP or body not in vi:
            if not kind.startswith('skip') and body not in SKIP and body not in vi:
                errors.append(f'missing translation: {f}: {body[:60]}')
            continue
        v = vi[body]
        used.add(body)
        if kind == 'plain':
            src = src[:s] + 'crate::i18n::tr("' + body + '")' + src[e:]
            continue
        # format! template
        m = re.search(r'(format|println|eprintln|panic)!\(\s*$', src[:s])
        if m.group(1) != 'format':
            continue
        if placeholders(body) != placeholders(v):
            errors.append(f'placeholder mismatch: {body!r} vs {v!r}')
            continue
        start = m.start()
        open_i = src.index('(', start)
        close_i = matching_paren(src, open_i)
        rest = src[e:close_i].strip()
        if rest.startswith(','):
            rest = rest[1:].strip()
        _, named = placeholders(body)
        extra = [f'{n} = {n}' for n in sorted(named) if not re.search(r'\b' + n + r'\s*=', rest)]
        args = ', '.join(x for x in [rest.rstrip(',').strip()] + extra if x)
        new = 'crate::ft!("' + body + '", "' + v + '"' + (', ' + args if args else '') + ')'
        src = src[:start] + new + src[close_i + 1:]
    open(f, 'w', encoding='utf-8').write(src)

# the lookup table
def esc(s):
    return s  # translations are written in Rust source form already

arms = []
for k, v in vi.items():
    if '{' in k and placeholders(k)[0] + len(placeholders(k)[1]) > 0 and k not in used:
        pass
    arms.append(f'        "{k}" => "{v}",')
table = '\n'.join(arms)
open('src/i18n.rs', 'w', encoding='utf-8').write(f'''//! Screen language: Korean (the source text) or Vietnamese.
//!
//! Fixed text goes through `tr("한국어")`; sentences with values in them use
//! `ft!("한국어 {{}}", "Tiếng Việt {{}}", value)`, so both versions are checked when building.
//! Generated from i18n_tools/vi.txt.

use std::sync::atomic::{{AtomicBool, Ordering}};

static VI: AtomicBool = AtomicBool::new(false);

pub fn set_vi(on: bool) {{
    VI.store(on, Ordering::Relaxed);
}}

pub fn vi() -> bool {{
    VI.load(Ordering::Relaxed)
}}

/// The text in the chosen language (unknown text stays Korean).
pub fn tr(s: &'static str) -> &'static str {{
    if !vi() {{
        return s;
    }}
    match s {{
{table}
        _ => s,
    }}
}}

#[macro_export]
macro_rules! ft {{
    ($ko:literal, $vi:literal) => {{
        if $crate::i18n::vi() {{ format!($vi) }} else {{ format!($ko) }}
    }};
    ($ko:literal, $vi:literal, $($rest:tt)*) => {{
        if $crate::i18n::vi() {{ format!($vi, $($rest)*) }} else {{ format!($ko, $($rest)*) }}
    }};
}}
''')
print('\n'.join(errors) if errors else 'no errors')
print('used', len(used), 'of', len(vi))
