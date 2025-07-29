from typing import TYPE_CHECKING

from sqlmodel import Field, Relationship

from .base import IdBase
from .meme_tag_link import MemeTagLink

if TYPE_CHECKING:
    from .tag import Tag

FILE_UNIQUE_ID = Field(min_length=1, max_length=255, unique=True)
FILE_ID = Field(min_length=1, max_length=255)


class Meme(IdBase, table=True):
    __tablename__ = "memes"  # type:ignore

    file_unique_id: str = FILE_UNIQUE_ID
    file_id: str = FILE_ID
    tags: list["Tag"] = Relationship(back_populates="memes", link_model=MemeTagLink)
