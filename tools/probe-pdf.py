import sys


def pdf_bytes(pages):
    objects = [(1, "<< /Type /Catalog /Pages 2 0 R >>")]
    font = 3 + len(pages) * 2
    kids = " ".join(f"{3 + at * 2} 0 R" for at in range(len(pages)))
    objects.append((2, f"<< /Type /Pages /Kids [{kids}] /Count {len(pages)} >>"))
    for at, lines in enumerate(pages):
        page = 3 + at * 2
        drawn = ("BT /F1 12 Tf 72 720 Td 16 TL\n"
                 + "".join(f"({line}) Tj T*\n" for line in lines) + "ET")
        objects.append((page, f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
                              f"/Resources << /Font << /F1 {font} 0 R >> >> "
                              f"/Contents {page + 1} 0 R >>"))
        objects.append((page + 1, f"<< /Length {len(drawn)} >>\nstream\n{drawn}\nendstream"))
    objects.append((font, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"))
    out = bytearray(b"%PDF-1.4\n")
    offsets = {}
    for number, body in objects:
        offsets[number] = len(out)
        out += f"{number} 0 obj\n{body}\nendobj\n".encode("ascii")
    started = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode("ascii")
    for number in range(1, len(objects) + 1):
        out += f"{offsets[number]:010d} 00000 n \n".encode("ascii")
    out += (f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\n"
            f"startxref\n{started}\n%%EOF\n").encode("ascii")
    return bytes(out)


pages = [["A letter from Green Lane", "Reference 4471"],
         ["Your appointment with the dentist is on Tuesday at half past nine.",
          "Please bring your insurance card, and tell us a day before if you cannot come."]]
data = pdf_bytes(pages)
open(sys.argv[1], "wb").write(data)
print(f"{len(data)} bytes")
