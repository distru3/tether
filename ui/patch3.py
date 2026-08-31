import re

with open('src/styles/app.css', 'r', encoding='utf-8') as f:
    css = f.read()

btn_pattern = r'\.btn \{\s*padding: 8px 14px;\s*border-radius: 0;\s*border: 1px solid transparent;\s*font-family: var\(--font-utility\);\s*font-size: 12px;\s*letter-spacing: 0\.1em;\s*text-transform: uppercase;\s*cursor: pointer;\s*transition: background-color 120ms ease, color 120ms ease;\s*\}'
btn_new = '.btn { padding: 8px 16px; border-radius: var(--radius-sm); border: 1px solid transparent; font-family: var(--font-sans); font-size: 13px; font-weight: 500; cursor: pointer; transition: all 120ms ease; display: inline-flex; align-items: center; gap: 8px; justify-content: center; }'
css = re.sub(btn_pattern, btn_new, css)

btn_primary_pattern = r'\.btn--primary \{\s*background: var\(--ink\);\s*border-color: var\(--ink\);\s*color: var\(--paper-raised\);\s*\}'
btn_primary_new = '.btn--primary { background: var(--text-primary); border-color: var(--text-primary); color: var(--bg-canvas); }'
css = re.sub(btn_primary_pattern, btn_primary_new, css)

btn_primary_hover_pattern = r'\.btn--primary:hover:not\(:disabled\) \{\s*background: #000000;\s*\}'
btn_primary_hover_new = '.btn--primary:hover:not(:disabled) { opacity: 0.9; }'
css = re.sub(btn_primary_hover_pattern, btn_primary_hover_new, css)

btn_secondary_pattern = r'\.btn--secondary \{\s*background: transparent;\s*border-color: var\(--rule-strong\);\s*color: var\(--ink\);\s*\}'
btn_secondary_new = '.btn--secondary { background: var(--bg-surface); border-color: var(--border-strong); color: var(--text-primary); }'
css = re.sub(btn_secondary_pattern, btn_secondary_new, css)

btn_secondary_hover_pattern = r'\.btn--secondary:hover:not\(:disabled\) \{\s*background: var\(--paper-deep\);\s*\}'
btn_secondary_hover_new = '.btn--secondary:hover:not(:disabled) { background: var(--bg-surface-hover); }'
css = re.sub(btn_secondary_hover_pattern, btn_secondary_hover_new, css)

with open('src/styles/app.css', 'w', encoding='utf-8') as f:
    f.write(css)
print("Updated buttons")
