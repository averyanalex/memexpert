from uuid import UUID

from sqlmodel import Field

from .base import TsBase


class MemeTagLink(TsBase, table=True):
    __tablename__ = "meme_tag_links"  # type:ignore

    meme_id: UUID = Field(foreign_key="memes.id", primary_key=True)

    tag_id: UUID = Field(foreign_key="tags.id", primary_key=True)
