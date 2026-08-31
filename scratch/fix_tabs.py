with open('ui/src/components/Sidebar.tsx', 'r') as f:
    text = f.read()

text = text.replace('" overview\ | \limits\ | \web-filtering\ | \settings\', '\overview\ | \limits\ | \web-filtering\ | \settings\')

# replace the focus nav item
old_nav = ''' <button
 type=\button\
 className={
av-item }
 onClick={() => onSelectTab(\web-filtering\)}
 >
 <ShieldIcon size={18} className=\nav-icon\ />
 <span className=\nav-text\>Web Filtering</span>
 {focusActive && <span className=\nav-pulse-dot\ title=\Focus session running\ />}
 </button>'''

new_nav = ''' <button
 type=\button\
 className={
av-item }
 onClick={() => onSelectTab(\web-filtering\)}
 >
 <ShieldIcon size={18} className=\nav-icon\ />
 <span className=\nav-text\>Web Filtering</span>
 </button>'''

text = text.replace(old_nav, new_nav)

with open('ui/src/components/Sidebar.tsx', 'w') as f:
 f.write(text)
