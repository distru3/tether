const fs = require('fs');
let content = fs.readFileSync('ui/src/App.tsx', 'utf8');

const target = '<div className="overview-activity-grid">';
const replacement = <div style={{ display: 'grid', gridTemplateColumns: '280px 1fr', gap: '24px' }}>\n                  <FocusWidget />\n                  <div className="overview-activity-grid">;

content = content.replace(target, replacement);

const target2 = </LedgerSection>\n                  </div>\n                )};
const replacement2 = </LedgerSection>\n                  </div>\n                )}\n                </div>;

content = content.replace(target2, replacement2);
fs.writeFileSync('ui/src/App.tsx', content);
