import io

from PIL import Image


def recode_image_to_jpeg(
    image_data: bytes, max_size: int = 512, quality: int = 90
) -> bytes:
    with Image.open(io.BytesIO(image_data)) as image:
        image = image.convert("RGB")

        if image.width > max_size or image.height > max_size:
            image.thumbnail((max_size, max_size), Image.Resampling.LANCZOS)

        output = io.BytesIO()
        image.save(output, format="JPEG", quality=quality)
        return output.getvalue()
