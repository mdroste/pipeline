#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

function makePdf(text) {
  const stream = `BT /F1 18 Tf 72 720 Td (${text.replace(/[()\\]/g, "\\$&")}) Tj ET\n`;
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    `<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}endstream`,
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
  ];
  let pdf = "%PDF-1.4\n";
  const offsets = [0];
  objects.forEach((object, index) => {
    offsets.push(Buffer.byteLength(pdf));
    pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;
  });
  const xref = Buffer.byteLength(pdf);
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets.slice(1)) pdf += `${String(offset).padStart(10, "0")} 00000 n \n`;
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return pdf;
}

function main(resourceDir) {
  const suffix = process.platform === "win32" ? ".exe" : "";
  const pdftotext = path.join(resourceDir, `pdftotext${suffix}`);
  const pdftoppm = path.join(resourceDir, `pdftoppm${suffix}`);
  for (const executable of [pdftotext, pdftoppm]) {
    if (!fs.existsSync(executable)) throw new Error(`missing ${executable}`);
  }

  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-poppler-smoke-"));
  try {
    const pdf = path.join(tempDir, "input.pdf");
    const textOutput = path.join(tempDir, "output.txt");
    const imagePrefix = path.join(tempDir, "page");
    fs.writeFileSync(pdf, makePdf("Pipeline release smoke test"));
    execFileSync(pdftotext, [pdf, textOutput], { stdio: "pipe", timeout: 30_000 });
    if (!fs.readFileSync(textOutput, "utf8").includes("Pipeline release smoke test")) {
      throw new Error("pdftotext did not extract the smoke-test sentence");
    }
    execFileSync(pdftoppm, ["-f", "1", "-l", "1", "-singlefile", "-png", pdf, imagePrefix], {
      stdio: "pipe",
      timeout: 30_000,
    });
    const png = fs.readFileSync(`${imagePrefix}.png`);
    if (!png.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
      throw new Error("pdftoppm did not produce a valid PNG");
    }
    console.log(`Poppler smoke test passed: ${resourceDir}`);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

try {
  const resourceDir = process.argv[2];
  if (!resourceDir) throw new Error("usage: smoke-poppler.mjs <poppler-resource-directory>");
  main(path.resolve(resourceDir));
} catch (error) {
  console.error(`Poppler smoke test failed: ${error.message}`);
  process.exitCode = 1;
}

