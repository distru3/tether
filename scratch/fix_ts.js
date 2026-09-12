
const fs = require("fs");
let app = fs.readFileSync("src/App.tsx", "utf8");
app = app.replace(/import \{ FocusWidget \} from ".\/components\/FocusWidget";\r?\n/, "");
fs.writeFileSync("src/App.tsx", app);

