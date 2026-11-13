import sys
sys.path.insert(0, 'i18n_tools')
from scan import literals

FILES = ['src/app.rs', 'src/ai_page.rs', 'src/analyze_page.rs', 'src/pipeline_page.rs', 'src/eval_ai.rs', 'src/ai.rs', 'src/cli.rs', 'src/sync.rs', 'src/local.rs']
seen = {}
skips = {}
for f in FILES:
    _, lits = literals(f)
    for s, e, body, kind in lits:
        if kind.startswith('skip'):
            skips.setdefault(kind, []).append((f, body))
            continue
        tag = 'F' if kind == 'fmt' else 'P'
        seen.setdefault(body, tag)
# extra UI strings that live in non-transformed files (wrapped by hand)
extra = sys.argv[1:] if len(sys.argv) > 1 else []
with open('i18n_tools/strings.txt', 'w', encoding='utf-8') as out:
    for body, tag in seen.items():
        out.write(f'{tag}\t{body}\n')
print('to translate:', len(seen), {k: len(v) for k, v in skips.items()})
for k, v in skips.items():
    for f, b in v[:40]:
        print(k, f, b[:50])
