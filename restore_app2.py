import json

base_path = r'ui\src\App.tsx'
content = ''

log_path = r'C:\Users\kacc2\.gemini\antigravity\brain\7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b\.system_generated\logs\transcript_full.jsonl'
with open(log_path, 'r', encoding='utf-8') as f:
    for line in f:
        try:
            entry = json.loads(line)
            if entry.get('type') == 'PLANNER_RESPONSE':
                for tc in entry.get('tool_calls', []):
                    if tc.get('name') in ['write_to_file', 'default_api:write_to_file']:
                        args = tc.get('args', {})
                        if 'ui/src/App.tsx' in args.get('TargetFile', '').replace('\\', '/'):
                            content = args.get('CodeContent', '')
                            print('Found write_to_file for App.tsx!')
                    if tc.get('name') in ['replace_file_content', 'default_api:replace_file_content']:
                        args = tc.get('args', {})
                        if 'ui/src/App.tsx' in args.get('TargetFile', '').replace('\\', '/'):
                            target = args.get('TargetContent', '')
                            repl = args.get('ReplacementContent', '')
                            if content and target in content:
                                content = content.replace(target, repl)
                            elif not content:
                                # We need to read the initial file if write_to_file wasn't used first
                                pass
        except Exception as e:
            pass

if content:
    with open(base_path, 'w', encoding='utf-8') as f:
        f.write(content)
        print('Restored from write_to_file!')
