from collections.abc import Mapping
from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="Meme")


@_attrs_define
class Meme:
    """
    Attributes:
        id (UUID):
        file_unique_id (str):
        file_id (str):
        tags (list[str]):
    """

    id: UUID
    file_unique_id: str
    file_id: str
    tags: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        id = str(self.id)

        file_unique_id = self.file_unique_id

        file_id = self.file_id

        tags = self.tags

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "id": id,
                "file_unique_id": file_unique_id,
                "file_id": file_id,
                "tags": tags,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        id = UUID(d.pop("id"))

        file_unique_id = d.pop("file_unique_id")

        file_id = d.pop("file_id")

        tags = cast(list[str], d.pop("tags"))

        meme = cls(
            id=id,
            file_unique_id=file_unique_id,
            file_id=file_id,
            tags=tags,
        )

        meme.additional_properties = d
        return meme

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
