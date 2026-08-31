import re

with open('src/styles/app.css', 'r', encoding='utf-8') as f:
    css = f.read()

# Make toasts impeccable
css = re.sub(r'\.toast \{\s*width: 100%;\s*padding: 10px 12px;\s*border: 1px solid var\(--rule-strong\);\s*border-left: 3px solid var\(--ink\);\s*border-radius: 0;\s*background: var\(--paper-raised\);\s*color: var\(--ink\);',
             '.toast { width: 100%; padding: 12px 16px; border: 1px solid var(--border-strong); border-radius: var(--radius-md); background: var(--bg-card-elevated); color: var(--text-primary); box-shadow: var(--shadow-sm);', css)
css = re.sub(r'\.toast--success \{\s*border-left-color: var\(--stamp-green\);\s*\}', '.toast--success { border: 1px solid var(--accent-emerald-border); }', css)
css = re.sub(r'\.toast--error \{\s*border-left-color: var\(--red\);\s*\}', '.toast--error { border: 1px solid var(--accent-rose-border); }', css)

with open('src/styles/app.css', 'w', encoding='utf-8') as f:
    f.write(css)
print("Updated toasts")
