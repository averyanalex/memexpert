from uuid import UUID

from pydantic import BaseModel, Field

from backend.models.meme import FILE_ID, FILE_UNIQUE_ID


class MemeCreate(BaseModel):
    file_id: str = FILE_ID
    file_unique_id: str = FILE_UNIQUE_ID
    file_data: str = Field(min_length=1, max_length=10 * 1024 * 1024)


class Meme(BaseModel):
    id: UUID
    file_unique_id: str
    file_id: str
    tags: list[str]
