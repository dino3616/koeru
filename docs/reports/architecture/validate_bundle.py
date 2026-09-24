#!/usr/bin/env python3
from pathlib import Path
import hashlib
import json
import re
import sys

root = Path(__file__).resolve().parent
errors = []
mds = sorted(root.glob('*.md'))

# Markdown structure and stale conclusions.
for p in mds:
    s = p.read_text()
    if s.count('```') % 2:
        errors.append(f'{p.name}: unbalanced code fences ({s.count("```")})')
    # Catch common editing damage where a heading was appended to prose/table text.
    for pat in (r'。#{2,3} ', r'\|#{2,3} '):
        if re.search(pat, s):
            errors.append(f'{p.name}: malformed heading boundary matching {pat!r}')

legacy_patterns = [
    r'GraphQL は採らない',
    r'code-first typed RPC',
    r'consumer-oriented DTO の code-first typed RPC',
    r'runtime::api definitions＋生成schema/catalog',
]
all_text = '\n'.join(p.read_text() for p in mds)
for pat in legacy_patterns:
    if re.search(pat, all_text):
        errors.append(f'legacy conclusion remains: {pat}')

# Required GraphQL contract assertions across the bundle.
required = {
    'README.md': ['canonical `schema.graphql`', 'GraphQL Subscription', 'ProjectReadSession'],
    '00-architecture-report.md': ['canonical schema.graphql', 'Query / Mutation / Subscription', 'ProjectReadSession', 'application-specific Tauri'],
    '01-contributor-guide.md': ['canonical GraphQL SDL', 'Subscription', 'featureからTauri invoke/Channel禁止'],
    '02-tests-observability.md': ['runtime schema', 'Subscription unsubscribe', 'persisted operation'],
    '03-task-dag.md': ['T03 — Canonical GraphQL SDL', 'T12 — Subscription unification', 'T16 — Coherent cutover'],
    '04-skills-design.md': ['Query/Mutation/Subscription/host/internal', 'canonical SDL↔runtime SDL parity'],
    '05-agent-handoff.md': ['canonical GraphQL SDL', 'Subscription unsubscribe'],
    '06-evidence.md': ['X09', 'X14'],
    '07-worked-examples.md': ['## H. 20Hz入力メーター', 'inputEnvelope'],
    '08-graphql-application-contract.md': ['Rust→React の application-visible', 'Channel<GraphqlExecutionResult>', 'persisted operations'],
    'QA.md': ['documentation/design artifact', 'GraphQL schema/resolver/codegen', '文書間の構造・方針整合'],
}
for fn, needles in required.items():
    p = root / fn
    if not p.exists():
        errors.append(f'missing {fn}')
        continue
    s = p.read_text()
    for n in needles:
        if n not in s:
            errors.append(f'{fn}: missing required text {n!r}')

# DAG JSON acyclic and references resolvable.
dag_path = root / 'task-dag.json'
if not dag_path.exists():
    errors.append('missing task-dag.json')
    order = []
else:
    try:
        j = json.loads(dag_path.read_text())
        pr = j['prerequisites']
    except Exception as e:
        errors.append(f'task-dag.json invalid: {e}')
        pr = {}
    remaining = set(pr)
    order = []
    while remaining:
        ready = sorted(x for x in remaining if all(d in order for d in pr[x]))
        if not ready:
            errors.append('task DAG cycle or unresolved dependency')
            break
        for x in ready:
            order.append(x)
            remaining.remove(x)
    for x, deps in pr.items():
        for d in deps:
            if d not in pr:
                errors.append(f'{x}: missing prerequisite node {d}')

# Task cards must have all 13 named fields for T00-T17.
task_md = root / '03-task-dag.md'
if task_md.exists():
    s = task_md.read_text()
    fields = [
        'Objective', 'Prerequisites', 'Architecture contract', 'Owned / affected',
        'Must not change', 'Tests', 'Observability', 'Migration / recovery',
        'Acceptance', 'Adversarial', 'Verification', 'Evidence outputs',
        'Human review / decision',
    ]
    for i in range(18):
        tid = f'T{i:02d}'
        m = re.search(rf'^## {tid} .*?(?=^## (?:T\d\d|H\d) |\Z)', s, re.M | re.S)
        if not m:
            errors.append(f'missing task card {tid}')
            continue
        card = m.group(0)
        for f in fields:
            if f'**{f}:**' not in card:
                errors.append(f'{tid}: missing field {f}')

if 'Tauri commandの実装型ではなく `runtime::api`' in all_text:
    errors.append('old runtime::api/Specta ownership remains')

# Manifest integrity. MANIFEST does not hash itself.
manifest_path = root / 'MANIFEST.json'
if not manifest_path.exists():
    errors.append('missing MANIFEST.json')
else:
    try:
        manifest = json.loads(manifest_path.read_text())
        entries = {e['path']: e for e in manifest['files']}
        actual_paths = sorted(
            p.name for p in root.iterdir()
            if p.is_file() and p.name != 'MANIFEST.json'
        )
        if sorted(entries) != actual_paths:
            missing = sorted(set(actual_paths) - set(entries))
            extra = sorted(set(entries) - set(actual_paths))
            errors.append(f'manifest file set mismatch missing={missing} extra={extra}')
        for name, e in entries.items():
            p = root / name
            if not p.exists():
                continue
            data = p.read_bytes()
            if e.get('bytes') != len(data):
                errors.append(f'{name}: manifest byte size mismatch')
            digest = hashlib.sha256(data).hexdigest()
            if e.get('sha256') != digest:
                errors.append(f'{name}: manifest sha256 mismatch')
    except Exception as e:
        errors.append(f'MANIFEST.json invalid: {e}')

print(json.dumps({
    'markdown_files': len(mds),
    'dag_order': order,
    'errors': errors,
}, ensure_ascii=False, indent=2))
sys.exit(1 if errors else 0)
