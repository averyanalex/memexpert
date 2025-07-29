from datetime import datetime
from uuid import UUID, uuid4

import sqlalchemy as sa
from sqlmodel import Field, SQLModel


class TsBase(SQLModel):
    created_at: datetime = Field(
        default=None,
        sa_type=sa.DateTime(timezone=True),  # type:ignore
        sa_column_kwargs={"server_default": sa.func.now()},
    )

    updated_at: datetime | None = Field(
        default=None,
        sa_type=sa.DateTime(timezone=True),  # type:ignore
        sa_column_kwargs={"onupdate": sa.func.now()},
    )


class IdBase(TsBase):
    id: UUID = Field(
        default_factory=uuid4,
        primary_key=True,
        sa_column_kwargs={"server_default": sa.func.gen_random_uuid()},
    )
