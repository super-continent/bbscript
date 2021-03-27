# BBScript

# [Releases Here](https://github.com/super-continent/bbscript/releases)

## What does this tool do?
This program is made to allow anyone to parse BBScript into a readable (and modifiable) format. It functions through game databases in [RON](https://github.com/ron-rs/ron) format, allowing it to be extended to work with any ArcSys game using BBScript regardless of the differences in functions and their corresponding values. This means it should be able to work with Guilty Gear Xrd, DBFZ, Blazblue, and Granblue Fantasy Versus if given the correct data in the form of .ron files.

## How do I get started with modding Guilty Gear?
To start modding, you're going to want to get a copy of [my other tool Rev2ModLoader](https://github.com/super-continent/Rev2ModLoader). Once you have that, you can load mods and rip the character scripts from the game! To rip scripts from memory, launch the mod loader and Guilty Gear, then click enable mods. Enter training mode or any game-mode, and then select which players script you want in the mod loader, click "Extract Script" and it should prompt you to write the file.

Once you have the script, simply open up cmd and run `bbscript parse ggrev2 <script file here> <readable output here>` and it should parse the script into a readable format! To rebuild the script into a usable format for the modloader, just run `bbscript rebuild ggrev2 <readeable script name here> <output usable file here>` and then place the script into your modloaders rev2_mods folder as CHARACTER_SHORTNAME.bbscript (shortnames listed at [Rev2ModLoaders README](https://github.com/super-continent/Rev2ModLoader))

As long as mods are enabled, it should work once you start a new match! Mods should work for online play if both players have the modloader and exact same scripts loaded in.

## What is BBScript?
BBScript is a script format used by Arc System Works to define functions in their games such as character moves. It's used in most modern ArcSys games such as Blazblue CentralFiction, Guilty Gear Xrd, Dragon Ball FighterZ, and Granblue Fantasy Versus.

## Credit
Thanks to Labryz and Dantarion for assembling the original DB info in [bbtools](https://github.com/dantarion/bbtools) and for bbtools as a good reference code base for info about the script format 

Special thanks to the quarantine for providing me with enough free time to waste a whole 2 weeks challenging myself to make something that like 5 people will think is actually cool
