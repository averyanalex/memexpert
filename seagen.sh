#!/bin/sh
rm -rv entities/src
sea-orm-cli generate entity -o entities/src -l
