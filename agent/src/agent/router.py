import base64

from dishka.integrations.fastapi import FromDishka, inject
from fastapi import APIRouter
from pydantic import BaseModel, Field
from pydantic_ai import Agent, BinaryContent

from .agent import TagsAgentAnswer, TagsAgentContext
from .images import recode_image_to_jpeg

router = APIRouter(prefix="/generate", tags=["generate"])


class GenerateTagsRequest(BaseModel):
    existing_tags: list[str] = Field(default_factory=list)
    image: str = Field(min_length=1, max_length=10 * 1024 * 1024)


class GenerateTagsResponse(BaseModel):
    tags: list[str]


@router.post("/tags", response_model=GenerateTagsResponse)
@inject
async def generate_tags(
    request: GenerateTagsRequest,
    tags_agent: FromDishka[Agent[TagsAgentContext, TagsAgentAnswer]],
):
    image_bytes = base64.b64decode(request.image)

    processed_image = recode_image_to_jpeg(image_bytes, max_size=512, quality=90)

    result = await tags_agent.run(
        [
            "Generate tags for the image",
            BinaryContent(data=processed_image, media_type="image/jpeg"),
        ],
        deps=TagsAgentContext(existing_tags=request.existing_tags),
    )
    return GenerateTagsResponse(tags=result.output.tags)
