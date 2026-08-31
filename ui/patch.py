import re

with open('src/styles/app.css', 'r', encoding='utf-8') as f:
    css = f.read()

# Replace all backdrop-filter
css = re.sub(r'backdrop-filter:\s*blur[^;]+;', '', css)

# Replace all background linear/radial gradients
css = re.sub(r'background(?:-image)?:\s*(?:linear|radial)-gradient[^;]+;', 'background: var(--bg-surface);', css)

# Fix border radii that are hardcoded > 10px
css = re.sub(r'border-radius:\s*[1-9][0-9]+px;', 'border-radius: var(--radius-md);', css)

# Write back
with open('src/styles/app.css', 'w', encoding='utf-8') as f:
    f.write(css)

print("Updated app.css")
