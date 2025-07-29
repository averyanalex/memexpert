from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="MemeCreate")


@_attrs_define
class MemeCreate:
    """
    Attributes:
        file_id (str):
        file_unique_id (str):
        file_data (str):
    """

    file_id: str
    file_unique_id: str
    file_data: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        file_id = self.file_id

        file_unique_id = self.file_unique_id

        file_data = self.file_data

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "file_id": file_id,
                "file_unique_id": file_unique_id,
                "file_data": file_data,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        file_id = d.pop("file_id")

        file_unique_id = d.pop("file_unique_id")

        file_data = d.pop("file_data")

        meme_create = cls(
            file_id=file_id,
            file_unique_id=file_unique_id,
            file_data=file_data,
        )

        meme_create.additional_properties = d
        return meme_create

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
