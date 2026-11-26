"""Regenerate the small Poppler qualification corpus (reportlab, pypdf, Pillow).

Fixtures are test inputs, never included in the desktop installer. Bitstream
Vera's bundled license is retained beside the embedded-font fixture.
"""
from io import BytesIO
from pathlib import Path
import json
import reportlab
from reportlab.pdfgen import canvas
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfbase.cidfonts import UnicodeCIDFont
from reportlab.lib.utils import ImageReader
from PIL import Image, ImageDraw
from pypdf import PdfReader, PdfWriter
from pypdf.generic import DictionaryObject, NameObject, NumberObject, BooleanObject, ArrayObject, DecodedStreamObject, EncodedStreamObject

root = Path(__file__).parent / "fixtures" / "poppler"
root.mkdir(parents=True, exist_ok=True)
fonts = Path(reportlab.__file__).parent / "fonts"
pdfmetrics.registerFont(TTFont("Vera", str(fonts / "Vera.ttf")))
for license in fonts.glob("*icense*"):
    (root / license.name).write_bytes(license.read_bytes())

c = canvas.Canvas(str(root / "text-layout.pdf"), invariant=1)
for number in range(1, 4):
    c.setFont("Helvetica", 18)
    c.drawString(55, 780, f"Pipeline PDF qualification page {number}")
    c.setFont("Times-Roman", 12)
    for row in range(8):
        c.drawString(55, 735 - row * 24, f"Research finding {row + 1}: 12.50 units")
        c.drawString(330, 735 - row * 24, f"Table value {row + 1}: 0.025")
    c.setFont("Symbol", 16)
    c.drawString(55, 470, "a + b = g")
    c.linkURL("https://example.com", (55, 700, 240, 720), relative=0)
    c.showPage()
c.save()

c = canvas.Canvas(str(root / "embedded-font.pdf"), invariant=1)
c.setFont("Vera", 16)
c.drawString(50, 760, "Résumé: naïve café — research data 12345")
c.drawString(50, 720, "Embedded TrueType font, ligatures and punctuation.")
c.save()

pdfmetrics.registerFont(UnicodeCIDFont("STSong-Light"))
c = canvas.Canvas(str(root / "cjk-cid.pdf"), invariant=1)
c.setFont("STSong-Light", 18)
c.drawString(50, 760, "研究方法 数据分析 日本語")
c.setFont("Helvetica", 12)
c.drawString(50, 720, "CJK character maps and Unicode extraction")
c.save()

image = Image.new("RGB", (240, 160), "white")
draw = ImageDraw.Draw(image)
draw.rectangle((15, 15, 225, 145), fill="navy")
draw.ellipse((50, 35, 190, 125), fill="orange")
draw.line((20, 140, 220, 20), fill="white", width=5)
c = canvas.Canvas(str(root / "images.pdf"), invariant=1)
c.setFont("Helvetica", 14)
c.drawString(50, 780, "JPEG and transparent PNG image test")
for index, fmt in enumerate(("JPEG", "PNG")):
    data = BytesIO()
    converted = image if fmt == "JPEG" else image.convert("RGBA")
    if fmt == "PNG":
        converted.putalpha(180)
    converted.save(data, format=fmt)
    c.drawImage(ImageReader(data), 50, 520 - index * 200, width=240, height=160, mask="auto")
c.save()

# Exercise Poppler's JPEG2000 decoder and CCITT Group 4 scanned-image path.
writer = PdfWriter()
for codec in ("JPXDecode", "CCITTFaxDecode"):
    page = writer.add_blank_page(width=300, height=220)
    data = BytesIO()
    source = image if codec == "JPXDecode" else image.convert("1")
    source.save(data, format="JPEG2000" if codec == "JPXDecode" else "TIFF", **({} if codec == "JPXDecode" else {"compression": "group4"}))
    raw = data.getvalue()
    if codec == "CCITTFaxDecode":
        tiff = Image.open(BytesIO(raw))
        offset, length = tiff.tag_v2[273][0], tiff.tag_v2[279][0]
        raw = raw[offset:offset + length]
    obj = EncodedStreamObject()
    obj._data = raw
    obj.update({NameObject("/Type"): NameObject("/XObject"), NameObject("/Subtype"): NameObject("/Image"), NameObject("/Width"): NumberObject(240), NameObject("/Height"): NumberObject(160), NameObject("/ColorSpace"): NameObject("/DeviceRGB" if codec == "JPXDecode" else "/DeviceGray"), NameObject("/BitsPerComponent"): NumberObject(8 if codec == "JPXDecode" else 1), NameObject("/Filter"): NameObject("/" + codec)})
    if codec == "CCITTFaxDecode":
        obj[NameObject("/DecodeParms")] = DictionaryObject({NameObject("/K"): NumberObject(-1), NameObject("/Columns"): NumberObject(240), NameObject("/Rows"): NumberObject(160), NameObject("/BlackIs1"): BooleanObject(False)})
    page[NameObject("/Resources")] = DictionaryObject({NameObject("/XObject"): DictionaryObject({NameObject("/Im0"): writer._add_object(obj)})})
    stream = DecodedStreamObject()
    stream.set_data(b"q 240 0 0 160 30 30 cm /Im0 Do Q")
    page[NameObject("/Contents")] = writer._add_object(stream)
writer.write(root / "scanned-codecs.pdf")

writer = PdfWriter()
page = writer.add_page(PdfReader(root / "text-layout.pdf").pages[0])
page.rotate(90)
page.cropbox.lower_left = (35, 400)
page.cropbox.upper_right = (560, 810)
writer.write(root / "rotated-cropped.pdf")

writer = PdfWriter()
writer.append(root / "embedded-font.pdf")
writer.encrypt("pipeline-test", algorithm="AES-256")
writer.write(root / "encrypted.pdf")
(root / "damaged.pdf").write_bytes(b"%PDF-1.7\nthis is intentionally damaged\n")
cases = [
    {"file": "text-layout.pdf", "text": "Pipeline PDF qualification page 1"},
    {"file": "embedded-font.pdf", "text": "Résumé"},
    {"file": "cjk-cid.pdf", "text": "研究方法"},
    {"file": "images.pdf", "text": "JPEG and transparent PNG"},
    {"file": "scanned-codecs.pdf", "text": ""},
    {"file": "rotated-cropped.pdf", "text": "Research finding"},
    {"file": "encrypted.pdf", "password": "pipeline-test", "text": "Résumé"},
    {"file": "encrypted.pdf", "fails": True},
    {"file": "damaged.pdf", "fails": True},
]
(root / "cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(f"Generated {len(cases)} cases in {root}")
