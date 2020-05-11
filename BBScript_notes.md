# File Structure

## General Stuff
Files extracted from the game UPKs have some Unreal Engine metadata at the start, 0x38 is the beginning of the actual script data, so if you're using extracted files, start reading at 0x38

Numbers are always little-endian

strings are either right-padded to 16 or 32 bytes, 16-byte strings are much rarer and seem to usually happen in cases where only the character short-name is needed or the string starts with vxxx

## Header

0x00: 4-byte Unsigned Int indicating the number of functions in the script
From 0x04 until `FUNCTION_COUNT * 0x24 + 0x04`: String indicating name of the state function that is always padded to 32 bytes, with a 4-byte unsigned int after it to indicate the offset of the state function relative to `FUNCTION_COUNT * 0x24 + 0x04`
