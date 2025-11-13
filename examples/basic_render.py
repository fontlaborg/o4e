#!/usr/bin/env python3
# this_file: examples/basic_render.py

"""Basic example of rendering text with o4e."""

import o4e
from pathlib import Path

def main():
    # Print version
    print(f"o4e version: {o4e.get_version()}")

    # Create a renderer with HarfBuzz backend
    renderer = o4e.TextRenderer(backend="harfbuzz")

    # Create a font specification
    # Use Helvetica which is available on macOS
    font = o4e.Font(
        family="/System/Library/Fonts/Helvetica.ttc",  # Full path to font
        size=48.0,
        weight=400,
        style="normal"
    )

    # Render some text
    text = "Hello, o4e!"
    print(f"Rendering: {text}")

    try:
        bitmap_data = renderer.render(text, font)
        print(f"Successfully rendered {len(bitmap_data)} bytes of bitmap data")

        # Save to file (raw RGBA data)
        output_file = Path("output.rgba")
        output_file.write_bytes(bytes(bitmap_data))
        print(f"Saved raw bitmap to {output_file}")

    except Exception as e:
        print(f"Error rendering: {e}")

    # Test different scripts
    test_texts = [
        ("Latin", "Hello World"),
        ("Cyrillic", "Привет мир"),
        ("Greek", "Γειά σου κόσμε"),
        ("CJK", "你好世界"),
        ("Arabic", "مرحبا بالعالم"),
    ]

    for script, text in test_texts:
        print(f"\nTesting {script}: {text}")
        try:
            bitmap_data = renderer.render(text, font)
            print(f"  ✓ Rendered {len(bitmap_data)} bytes")
        except Exception as e:
            print(f"  ✗ Failed: {e}")

if __name__ == "__main__":
    main()