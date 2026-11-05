#!/usr/bin/env python3
"""Build the folder that file-browser-native-check.mjs opens.

The PDF is sparse: over 1 GiB on paper, a few kilobytes on disk.
"""
import json
import math
import struct
import sys
import wave
import zipfile
import zlib
from pathlib import Path

ROOT = Path(sys.argv[1])
ROOT.mkdir(parents=True, exist_ok=True)
(ROOT / "code").mkdir(exist_ok=True)
(ROOT / "code" / "run.do").write_text("regress wage education\n")
(ROOT / "notes.md").write_text("# Notes\n\nFirst version.\n")
(ROOT / "data.csv").write_text("name,value\n" + "".join(f"r{i},{i}\n" for i in range(450)))
with (ROOT / "panel.dta").open("wb") as dataset:
    dataset.write(b"<stata_dta><header>")
    dataset.truncate(2 * 1024**3)
(ROOT / "unsafe.html").write_text(
    "<html><head><style>h1{color:#0a7}</style></head><body><h1>Styled heading</h1>"
    "<script>parent.__hacked=true;parent.document.title='hacked';document.body.innerHTML='ran'</script>"
    "<img src=https://example.com/track.png></body></html>"
)
(ROOT / "analysis.ipynb").write_text(json.dumps({
    "metadata": {"language_info": {"name": "python"}},
    "cells": [
        {"cell_type": "markdown", "source": ["# Analysis\n"]},
        {"cell_type": "code", "execution_count": 1, "source": ["print(1)"],
         "outputs": [{"output_type": "stream", "name": "stdout", "text": ["1\n"]}]},
    ],
}))


def png(width, height):
    rows = b"".join(
        b"\0" + bytes(c for x in range(width) for c in ((x * 255) // width, (y * 255) // height, 128))
        for y in range(height)
    )
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


(ROOT / "figure.png").write_bytes(png(640, 480))


def tiff(width, height):
    pixels = bytes(c for y in range(height) for x in range(width) for c in ((x * 255) // width, 90, (y * 255) // height))
    entries = [(256, 4, 1, width), (257, 4, 1, height), (258, 3, 3, 8 + 2 + 9 * 12 + 4), (259, 3, 1, 1),
               (262, 3, 1, 2), (273, 4, 1, 8 + 2 + 9 * 12 + 4 + 6), (277, 3, 1, 3), (278, 4, 1, height),
               (279, 4, 1, len(pixels))]
    out = b"II*\0" + struct.pack("<I", 8) + struct.pack("<H", len(entries))
    for tag, kind, count, value in entries:
        out += struct.pack("<HHII", tag, kind, count, value)
    return out + struct.pack("<I", 0) + struct.pack("<HHH", 8, 8, 8) + pixels


(ROOT / "scan.tif").write_bytes(tiff(320, 240))

with wave.open(str(ROOT / "tone.wav"), "wb") as audio:
    audio.setnchannels(1)
    audio.setsampwidth(2)
    audio.setframerate(22050)
    audio.writeframes(b"".join(
        struct.pack("<h", int(12000 * math.sin(i * 440 * 2 * math.pi / 22050))) for i in range(22050 * 8)
    ))

with zipfile.ZipFile(ROOT / "book.xlsx", "w") as book:
    book.writestr("[Content_Types].xml", '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>')
    book.writestr("_rels/.rels", '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>')
    book.writestr("xl/workbook.xml", '<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Results" sheetId="1" r:id="rId1"/></sheets></workbook>')
    book.writestr("xl/_rels/workbook.xml.rels", '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>')
    book.writestr("xl/worksheets/sheet1.xml", '<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>term</t></is></c><c r="B1" t="inlineStr"><is><t>beta</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>education</t></is></c><c r="B2"><v>0.08</v></c></row></sheetData></worksheet>')

with zipfile.ZipFile(ROOT / "report.docx", "w") as document:
    document.writestr("[Content_Types].xml", '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
    document.writestr("_rels/.rels", '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
    document.writestr("word/document.xml", '<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>A short report for the viewer.</w:t></w:r></w:p></w:body></w:document>')


def huge_pdf(path, padding):
    pages = 3
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [" + b" ".join(b"%d 0 R" % (3 + i * 2) for i in range(pages)) + b"] /Count %d >>" % pages,
    }
    for i in range(pages):
        text = b"BT /F1 24 Tf 72 700 Td (Page %d of a very large file) Tj ET" % (i + 1)
        objects[3 + i * 2] = (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents %d 0 R "
                              b"/Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>" % (4 + i * 2))
        objects[4 + i * 2] = b"<< /Length %d >>\nstream\n%s\nendstream" % (len(text), text)
    padding_id = 3 + pages * 2
    with open(path, "wb") as out:
        out.write(b"%PDF-1.7\n")
        offsets = {}
        for number, body in sorted(objects.items()):
            offsets[number] = out.tell()
            out.write(b"%d 0 obj\n%s\nendobj\n" % (number, body))
        offsets[padding_id] = out.tell()
        out.write(b"%d 0 obj\n<< /Length %d >>\nstream\n" % (padding_id, padding))
        out.seek(padding, 1)
        out.write(b"\nendstream\nendobj\n")
        start = out.tell()
        out.write(b"xref\n0 %d\n0000000000 65535 f \n" % (padding_id + 1))
        for number in range(1, padding_id + 1):
            out.write(b"%010d 00000 n \n" % offsets[number])
        out.write(b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (padding_id + 1, start))


huge_pdf(ROOT / "huge.pdf", 1100 * 1024 * 1024)
print(ROOT)
