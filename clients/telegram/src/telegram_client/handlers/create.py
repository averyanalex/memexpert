from aiogram import Bot, F, Router
from aiogram.enums import ChatAction
from aiogram.types import Message
from aiogram.utils.formatting import Bold, Text, as_marked_list
from dishka.integrations.aiogram import FromDishka

from telegram_client.clients import BackendClient

create_router = Router()


@create_router.message(F.photo)
async def photo_handler(
    message: Message, bot: Bot, backend_client: FromDishka[BackendClient]
) -> None:
    await bot.send_chat_action(message.chat.id, ChatAction.TYPING)

    photo_sizes = message.photo
    assert photo_sizes is not None
    photo = photo_sizes[-1]

    file_id = photo.file_id
    file_unique_id = photo.file_unique_id

    file_info = await bot.get_file(file_id)
    file_path = file_info.file_path
    assert file_path is not None
    file_bytes = await bot.download_file(file_path)
    assert file_bytes is not None

    meme = await backend_client.create_meme(file_id, file_unique_id, file_bytes.read())

    response = Text(
        Bold("Мем создан!"),
        "\n\n",
        "Сгенерированные теги:\n",
        as_marked_list(*meme.tags, marker="• "),
    )

    await message.answer(**response.as_kwargs())
