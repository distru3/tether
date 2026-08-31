with open('ui/src/components/WebFilteringPanel.tsx', 'r') as f:
    text = f.read()

text = text.replace('className=" input\', 'className=\form-input\')
text = text.replace('className=\button primary\', 'className=\btn btn-primary\')
text = text.replace('className=\button danger small\', 'className=\btn btn-ghost text-danger btn-sm\')

with open('ui/src/components/WebFilteringPanel.tsx', 'w') as f:
 f.write(text)
