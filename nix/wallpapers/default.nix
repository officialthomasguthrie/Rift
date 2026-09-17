# the photographs the desktop can have as its wallpaper. each one comes from nasa's image library by
# its hash, is scaled down to at most 3840 pixels wide and saved as a jpeg, and gets a text file next
# to it that says what it shows, who took it, where it came from and on what terms. the files are
# fetched when the image is built rather than kept in git, the way the models are: a clone stays
# small, and a photograph that is replaced leaves nothing behind in the history
{
  lib,
  runCommand,
  writeText,
  fetchurl,
  vips,
}:
let
  # nasa's words for its own photographs, and the page that says so
  license = "Not subject to copyright in the United States: https://www.nasa.gov/nasa-brand-center/images-and-media/";

  photos = [
    {
      name = "dark-side-of-earth";
      title = "The dark side of Earth";
      credit = "NASA/Reid Wiseman";
      taken = "April 3, 2026, from a window of the Orion spacecraft on Artemis II, after the burn that sent it to the Moon";
      id = "art002e000193";
      hash = "sha256-m4cENAUUriDbnDcjBRcDasrwFGSToIUI/s0dgWjAVOQ=";
    }
    {
      name = "earthset";
      title = "Earthset over the Moon";
      credit = "NASA";
      taken = "April 6, 2026, by the crew of Artemis II during the lunar flyby";
      id = "art002e009284";
      hash = "sha256-04QrgjpFpV00JRnJ81mXjKQR2XMMoKbSes/ACyZpbOA=";
    }
    {
      name = "crescent-earth";
      title = "Crescent Earth";
      credit = "NASA";
      taken = "April 3, 2026, from a window of the Orion spacecraft on Artemis II";
      id = "art002e004437";
      hash = "sha256-QEOpeXN7ehFNi4RBz7dZ+57l9UmC4f5O/j4Rc/N2YA8=";
    }
    {
      name = "milky-way";
      title = "The Milky Way from Orion";
      credit = "NASA";
      taken = "April 7, 2026, by the crew of Artemis II";
      id = "art002e012588";
      hash = "sha256-aciR67pKtedCh6BbooZz4P8OzuZ7J3ieipJTkEgZrBA=";
    }
    {
      name = "earth-from-orion";
      title = "Earth from Orion";
      credit = "NASA";
      taken = "April 2, 2026, from a window of the Orion spacecraft on Artemis II";
      id = "art002e023575";
      hash = "sha256-G/SS6n8AtRJ3RADIBR0tZT6DTNO7zzANFSnEosk00q0=";
    }
    {
      name = "airglow";
      title = "Red airglow under the Milky Way";
      credit = "NASA";
      taken = "October 25, 2025, from the International Space Station over the Arabian Sea";
      id = "iss073e0982696";
      hash = "sha256-fUNe69lnsbGRjn5HHyzGRRdQHBx9y5el3cORLuELKF8=";
    }
    {
      name = "aurora";
      title = "Aurora borealis over Manitoba";
      credit = "NASA";
      taken = "October 30, 2024, from the International Space Station";
      id = "iss072e159172";
      hash = "sha256-8HK/GR9e7JzrlFh0S4KLyo19nkqfz+tosMjbm86vgqk=";
    }
    {
      name = "perseids";
      title = "A Perseid meteor over Spruce Knob";
      credit = "NASA/Bill Ingalls";
      taken = "August 10, 2021, in a 30 second exposure at Spruce Knob, West Virginia";
      id = "NHQ202108100009";
      hash = "sha256-xoPtZbCsuEE5IQz/KpMSfLwn4SsRTSTqyGOiHiIi3Tk=";
    }
    {
      name = "diamond-ring";
      title = "The diamond ring of the 2017 total solar eclipse";
      credit = "NASA/Aubrey Gemignani";
      taken = "August 21, 2017, above Madras, Oregon";
      id = "NHQ201708210102";
      # the library keeps this one as a tiff
      extension = "tif";
      hash = "sha256-72+SPHnHk5+6R4nyXXjKLpkoQmqXwoTCe2hIgwuGJkI=";
    }
  ];

  original =
    photo:
    fetchurl {
      url = "https://images-assets.nasa.gov/image/${photo.id}/${photo.id}~orig.${photo.extension or "jpg"}";
      # a store path cannot have the ~ of the file's own name
      name = "${photo.id}.${photo.extension or "jpg"}";
      inherit (photo) hash;
    };

  about =
    photo:
    writeText "${photo.name}.txt" ''
      Title: ${photo.title}
      Credit: ${photo.credit}
      Taken: ${photo.taken}
      Source: https://images.nasa.gov/details/${photo.id}
      License: ${license}
      Changes: scaled down to at most 3840 pixels wide and saved as JPEG
    '';
in
runCommand "rift-wallpapers"
  {
    nativeBuildInputs = [ vips ];
    # the one the desktop has until the owner picks another
    passthru.default = "dark-side-of-earth";
    passthru.names = map (photo: photo.name) photos;
  }
  ''
    folder=$out/share/backgrounds/rift
    mkdir -p $folder
    ${lib.concatMapStrings (photo: ''
      vips thumbnail ${original photo} "$folder/${photo.name}.jpg[Q=90,keep=none,optimize_coding]" 3840 --height 3840 --size down
      cp ${about photo} $folder/${photo.name}.txt
    '') photos}
  ''
