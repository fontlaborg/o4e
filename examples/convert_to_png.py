#!/usr/bin/env python3
# this_file: examples/convert_to_png.py

"""Convert raw RGBA data to PNG for visualization."""

import struct
from PIL import Image
from pathlib import Path
import sys

def convert_rgba_to_png(input_file="output.rgba", output_file="output.png"):
    """Convert raw RGBA data to PNG."""
    # Read the raw data
    raw_data = Path(input_file).read_bytes()

    # The data length should be divisible by 4 (RGBA)
    if len(raw_data) % 4 != 0:
        print(f"Error: Raw data size {len(raw_data)} is not divisible by 4")
        return False

    # Try to guess dimensions (assuming roughly square)
    pixel_count = len(raw_data) // 4

    # Try some common dimensions based on the test output
    # 175200 bytes = 43800 pixels
    test_dimensions = [
        (365, 120),  # 43800 pixels
        (312, 150),  # 46800 pixels
        (260, 180),  # 46800 pixels
        (300, 146),  # 43800 pixels
    ]

    # Find dimension that matches
    width, height = 0, 0
    for w, h in test_dimensions:
        if w * h == pixel_count:
            width, height = w, h
            break

    if width == 0:
        # Try to factor the pixel count
        import math
        sqrt_pixels = int(math.sqrt(pixel_count))
        for h in range(sqrt_pixels, 0, -1):
            if pixel_count % h == 0:
                width = pixel_count // h
                height = h
                break

    if width == 0:
        print(f"Could not determine dimensions for {pixel_count} pixels")
        return False

    print(f"Converting {input_file} ({pixel_count} pixels) to {output_file} ({width}x{height})")

    # Create image from raw RGBA data
    img = Image.frombytes('RGBA', (width, height), raw_data)

    # Save as PNG
    img.save(output_file)
    print(f"Saved PNG to {output_file}")
    return True

if __name__ == "__main__":
    input_file = sys.argv[1] if len(sys.argv) > 1 else "output.rgba"
    output_file = sys.argv[2] if len(sys.argv) > 2 else "output.png"
    convert_rgba_to_png(input_file, output_file)