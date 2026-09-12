
const fs = require("fs");
const lines = fs.readFileSync("C:\\Users\\kacc2\\.gemini\\antigravity\\brain\\7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b\\.system_generated\\logs\\transcript_full.jsonl", "utf8").split("\n");
for (const line of lines) {
    if (!line) continue;
    try {
        const obj = JSON.parse(line);
        if (obj.step_index === 12299 && obj.content) {
            // Find where "diff --git" starts
            const content = obj.content;
            const diffStart = content.indexOf("diff --git");
            if (diffStart !== -1) {
                fs.writeFileSync("scratch/user_changes.patch", content.substring(diffStart));
            }
        }
    } catch (e) {}
}

