import json

base_path = r'ui\src\App.tsx'
with open(base_path, 'r', encoding='utf-8') as f:
    content = f.read()

log_path = r'C:\Users\kacc2\.gemini\antigravity\brain\7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b\.system_generated\logs\transcript_full.jsonl'
applied = 0
with open(log_path, 'r', encoding='utf-8') as f:
    for line in f:
        try:
            entry = json.loads(line)
            if entry.get('type') == 'PLANNER_RESPONSE':
                for tc in entry.get('tool_calls', []):
                    if tc.get('name') in ['replace_file_content', 'default_api:replace_file_content']:
                        args = tc.get('args', {})
                        target_file = args.get('TargetFile', '')
                        if 'ui/src/App.tsx' in target_file.replace('\\', '/'):
                            target = args.get('TargetContent', '')
                            repl = args.get('ReplacementContent', '')
                            if target in content:
                                content = content.replace(target, repl)
                                applied += 1
                            else:
                                print('Warning: target not found in App.tsx')
        except Exception as e:
            pass

with open(base_path, 'w', encoding='utf-8') as f:
    f.write(content)
print(f'Applied {applied} patches to App.tsx')
