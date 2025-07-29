from typing import TYPE_CHECKING

from sqlmodel import Field, Relationship

from .base import IdBase
from .meme_tag_link import MemeTagLink

if TYPE_CHECKING:
    from .meme import Meme


NAME = Field(min_length=1, max_length=255, unique=True)


class Tag(IdBase, table=True):
    __tablename__ = "tags"  # type:ignore

    name: str = NAME
    memes: list["Meme"] = Relationship(back_populates="tags", link_model=MemeTagLink)
