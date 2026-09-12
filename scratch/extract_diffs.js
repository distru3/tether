
const fs = require("fs");
const lines = fs.readFileSync("C:\\Users\\kacc2\\.gemini\\antigravity\\brain\\7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b\\.system_generated\\logs\\transcript_full.jsonl", "utf8").split("\n");
for (const line of lines) {
    if (!line) continue;
    try {
        const obj = JSON.parse(line);
        if (obj.content && obj.content.includes("diff --git")) {
            console.log("Found diff in step", obj.step_index);
            fs.appendFileSync("scratch/extracted_diff.txt", "\n\nSTEP " + obj.step_index + "\n" + obj.content);
        }
    } catch (e) {}
}

