#!/usr/bin/env python3
# this_file: examples/test_png_output.py

"""Test PNG output format from o4e."""

import o4e
from pathlib import Path

def test_png_output():
    """Test rendering text directly to PNG."""
    print(f"o4e version: {o4e.get_version()}")

    # Create a renderer
    renderer = o4e.TextRenderer(backend="harfbuzz")

    # Create a font
    font = o4e.Font(
        family="/System/Library/Fonts/Helvetica.ttc",
        size=48.0,
        weight=400,
        style="normal"
    )

    # Test texts
    texts = [
        ("Hello o4e!", "hello.png"),
        ("Testing PNG", "test.png"),
        ("你好世界", "chinese.png"),
        ("مرحبا", "arabic.png"),
    ]

    for text, filename in texts:
        print(f"Rendering '{text}' to {filename}...")
        try:
            # Request PNG format
            result = renderer.render(text, font, output_format="png")

            if isinstance(result, (bytes, bytearray, list)):
                # Convert to bytes if needed
                if isinstance(result, list):
                    result = bytes(result)
                # Save the PNG data
                Path(filename).write_bytes(result)
                print(f"  ✓ Saved {len(result)} bytes to {filename}")
            else:
                print(f"  Got unexpected result type: {type(result)}")
        except Exception as e:
            print(f"  ✗ Error: {e}")

if __name__ == "__main__":
    test_png_output()