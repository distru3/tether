
const Tesseract = require("tesseract.js");
const fs = require("fs");
const path = require("path");

const dir = "C:/Users/kacc2/.gemini/antigravity/brain/7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b/.user_uploaded/";
const files = fs.readdirSync(dir).filter(f => f.endsWith(".png") || f.endsWith(".jpg"));

async function run() {
  for (const file of files) {
    const fullPath = path.join(dir, file);
    try {
      const { data: { text } } = await Tesseract.recognize(fullPath, "eng");
      console.log(`\n--- ${file} ---`);
      console.log(text.substring(0, 500));
    } catch (e) {
      console.log(`Error reading ${file}`);
    }
  }
}
run();

