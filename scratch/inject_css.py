import re

css_path = 'ui/src/styles/app.css'
with open(css_path, 'r', encoding='utf-8') as f:
    css = f.read()

# 1. Add background mesh to body
body_pattern = r'(body\s*\{[^}]*\})'
mesh_bg = '''body {
  background-color: var(--bg-canvas);
  background-image: 
    radial-gradient(at 100% 0%, oklch(0.65 0.18 280 / 0.15) 0px, transparent 40%),
    radial-gradient(at 0% 100%, oklch(0.7 0.15 220 / 0.15) 0px, transparent 40%);
  background-attachment: fixed;
  color: var(--text-primary);
  font-family: var(--font-sans);
  font-size: 15px;
  line-height: 1.55;
  scrollbar-gutter: stable;
  margin: 0;
}'''
css = re.sub(r'body\s*\{[^}]*\}', mesh_bg, css, count=1)

# 2. Add glassmorphism to sidebar
sidebar_pattern = r'\.app-sidebar\s*\{([^}]*)\}'
def sidebar_repl(m):
    content = m.group(1)
    content = re.sub(r'background-color:[^;]+;', 'background-color: var(--bg-sidebar);\n  backdrop-filter: blur(24px);\n  -webkit-backdrop-filter: blur(24px);', content)
    return '.app-sidebar {' + content + '}'
css = re.sub(sidebar_pattern, sidebar_repl, css)

# 3. Add glassmorphism to card
card_pattern = r'(\.card\s*\{[^}]*background-color:[^}]*\})'
def card_repl(m):
    return m.group(1).replace('background-color: var(--bg-card);', 'background-color: var(--bg-card);\n  backdrop-filter: blur(12px);\n  -webkit-backdrop-filter: blur(12px);')
css = re.sub(card_pattern, card_repl, css)

# 4. Add @starting-style and discrete transition to .dialog
dialog_transition = '''
.dialog {
  transition: opacity var(--transition-normal), transform var(--transition-bounce), display var(--transition-normal);
  transition-behavior: allow-discrete;
}

@starting-style {
  .dialog {
    opacity: 0;
    transform: scale(0.95) translateY(10px);
  }
}

.dialog[open] {
  opacity: 1;
  transform: scale(1) translateY(0);
}
'''
if '@starting-style' not in css:
    css = css.replace('.dialog {', dialog_transition + '\n.dialog_base {')

# 5. Make nav-item buttons scale on active
css = css.replace('.nav-item {', '.nav-item {\n  transform-origin: center;\n')
if '.nav-item:active' not in css:
    css = css.replace('.nav-item:hover {', '.nav-item:active {\n  transform: scale(0.96);\n}\n\n.nav-item:hover {')

# 6. Add view transition basic animation
view_transition = '''
/* ---- View Transitions ---- */
::view-transition-old(root),
::view-transition-new(root) {
  animation-duration: 0.3s;
  animation-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

::view-transition-new(root) {
  animation-name: fade-in-scale;
}

::view-transition-old(root) {
  animation-name: fade-out-scale;
}

@keyframes fade-in-scale {
  from { opacity: 0; transform: scale(0.98); }
  to { opacity: 1; transform: scale(1); }
}

@keyframes fade-out-scale {
  from { opacity: 1; transform: scale(1); }
  to { opacity: 0; transform: scale(1.02); }
}
'''
if '::view-transition' not in css:
    css += '\n' + view_transition

with open(css_path, 'w', encoding='utf-8') as f:
    f.write(css)
print( Injected CSS!)
