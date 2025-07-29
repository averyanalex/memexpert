from collections.abc import Mapping
from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="GenerateTagsRequest")


@_attrs_define
class GenerateTagsRequest:
    """
    Attributes:
        image (str):
        existing_tags (Union[Unset, list[str]]):
    """

    image: str
    existing_tags: Union[Unset, list[str]] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        image = self.image

        existing_tags: Union[Unset, list[str]] = UNSET
        if not isinstance(self.existing_tags, Unset):
            existing_tags = self.existing_tags

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "image": image,
            }
        )
        if existing_tags is not UNSET:
            field_dict["existing_tags"] = existing_tags

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        image = d.pop("image")

        existing_tags = cast(list[str], d.pop("existing_tags", UNSET))

        generate_tags_request = cls(
            image=image,
            existing_tags=existing_tags,
        )

        generate_tags_request.additional_properties = d
        return generate_tags_request

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
