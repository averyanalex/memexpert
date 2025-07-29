from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.http_validation_error import HTTPValidationError
from ...models.meme import Meme
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    text: Union[None, Unset, str] = UNSET,
    limit: Union[Unset, int] = 10,
) -> dict[str, Any]:
    params: dict[str, Any] = {}

    json_text: Union[None, Unset, str]
    if isinstance(text, Unset):
        json_text = UNSET
    else:
        json_text = text
    params["text"] = json_text

    params["limit"] = limit

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/memes",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[HTTPValidationError, list["Meme"]]]:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = Meme.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200
    if response.status_code == 422:
        response_422 = HTTPValidationError.from_dict(response.json())

        return response_422
    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Response[Union[HTTPValidationError, list["Meme"]]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    text: Union[None, Unset, str] = UNSET,
    limit: Union[Unset, int] = 10,
) -> Response[Union[HTTPValidationError, list["Meme"]]]:
    """Search Memes

    Args:
        text (Union[None, Unset, str]):
        limit (Union[Unset, int]):  Default: 10.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[HTTPValidationError, list['Meme']]]
    """

    kwargs = _get_kwargs(
        text=text,
        limit=limit,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: Union[AuthenticatedClient, Client],
    text: Union[None, Unset, str] = UNSET,
    limit: Union[Unset, int] = 10,
) -> Optional[Union[HTTPValidationError, list["Meme"]]]:
    """Search Memes

    Args:
        text (Union[None, Unset, str]):
        limit (Union[Unset, int]):  Default: 10.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[HTTPValidationError, list['Meme']]
    """

    return sync_detailed(
        client=client,
        text=text,
        limit=limit,
    ).parsed


async def asyncio_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    text: Union[None, Unset, str] = UNSET,
    limit: Union[Unset, int] = 10,
) -> Response[Union[HTTPValidationError, list["Meme"]]]:
    """Search Memes

    Args:
        text (Union[None, Unset, str]):
        limit (Union[Unset, int]):  Default: 10.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[HTTPValidationError, list['Meme']]]
    """

    kwargs = _get_kwargs(
        text=text,
        limit=limit,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: Union[AuthenticatedClient, Client],
    text: Union[None, Unset, str] = UNSET,
    limit: Union[Unset, int] = 10,
) -> Optional[Union[HTTPValidationError, list["Meme"]]]:
    """Search Memes

    Args:
        text (Union[None, Unset, str]):
        limit (Union[Unset, int]):  Default: 10.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[HTTPValidationError, list['Meme']]
    """

    return (
        await asyncio_detailed(
            client=client,
            text=text,
            limit=limit,
        )
    ).parsed
