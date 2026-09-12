
const fs = require("fs");
let css = fs.readFileSync("ui/src/styles/redesign.css", "utf8");
css = css.replace("minmax(280px, 0.8fr) minmax(0, 1.38fr) minmax(280px, 0.72fr)", "minmax(0, 1.38fr) minmax(280px, 0.72fr)");
// Re-insert the duplicate rule that I deleted, just before .timeline-panel {
css = css.replace("\n\n.timeline-panel {", "\n\n.overview-activity-grid {\n  grid-template-columns: minmax(0, 1.42fr) minmax(290px, 0.78fr);\n}\n\n.timeline-panel {");
fs.writeFileSync("ui/src/styles/redesign.css", css);

