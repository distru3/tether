import re
with open('ui/src/components/Sidebar.tsx', 'r') as f:
    text = f.read()

text = text.replace('web-filteringIcon', 'FocusIcon')
text = text.replace('web-filteringActive', 'focusActive')

with open('ui/src/components/Sidebar.tsx', 'w') as f:
    f.write(text)
