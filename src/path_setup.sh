#!/run/current-system/sw/bin/bash

# Initialize an empty PKG_CONFIG_PATH
PKG_CONFIG_PATH=""

# Initialize an empty LD_LIBRARY_PATH

# Search for alsa.pc and add its pkgconfig directory to PKG_CONFIG_PATH
alsa_path=$(find /nix/store -name 'alsa.pc' 2>/dev/null | head -n 1)
if [ ! -z "$alsa_path" ]; then
    alsa_pkgconfig_dir=$(dirname $alsa_path)
    PKG_CONFIG_PATH="$PKG_CONFIG_PATH:$alsa_pkgconfig_dir"
fi

# Search for libudev.pc and add its pkgconfig directory to PKG_CONFIG_PATH
libudev_path=$(find /nix/store -name 'libudev.pc' 2>/dev/null | head -n 1)
if [ ! -z "$libudev_path" ]; then
    libudev_pkgconfig_dir=$(dirname $libudev_path)
    PKG_CONFIG_PATH="$PKG_CONFIG_PATH:$libudev_pkgconfig_dir"
fi

# Search for libX11.so.6 and add its containing directory to LD_LIBRARY_PATH
libX11_path=$(find /nix/store -name 'libX11.so.6' 2>/dev/null | head -n 1)
if [ ! -z "$libX11_path" ]; then
    libX11_dir=$(dirname $libX11_path)
    LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$libX11_dir"
fi

# Search for libXcursor.so.1 and add its containing directory to LD_LIBRARY_PATH
libXcursor_path=$(find /nix/store -name 'libXcursor.so.1' 2>/dev/null | head -n 1)
if [ ! -z "$libXcursor_path" ]; then
    libXcursor_dir=$(dirname $libXcursor_path)
    LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$libXcursor_dir"
fi

# Search for libXrandr.so.2 and add its containing directory to LD_LIBRARY_PATH
libXrandr_path=$(find /nix/store -name 'libXrandr.so.2' 2>/dev/null | head -n 1)
if [ ! -z "$libXcursor_path" ]; then
    libXrandr_dir=$(dirname $libXrandr_path)
    LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$libXrandr_dir"
fi

# Search for libXi.so and add its containing directory to LD_LIBRARY_PATH
libXi_path=$(find /nix/store -name 'libXi.so' 2>/dev/null | head -n 1)
if [ ! -z "$libXi_path" ]; then
    libXi_dir=$(dirname $libXi_path)
    LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$libXi_dir"
fi

# Search for libGL.so and add its containing directory to LD_LIBRARY_PATH
libGL_path=$(find /nix/store -name 'libGL.so' 2>/dev/null | head -n 1)
if [ ! -z "$libGL_path" ]; then
    libGL_dir=$(dirname $libGL_path)
    LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$libGL_dir"
fi

# Remove leading colon if it exists in PKG_CONFIG_PATH
PKG_CONFIG_PATH=${PKG_CONFIG_PATH#:}

# Remove leading colon if it exists in LD_LIBRARY_PATH
LD_LIBRARY_PATH=${LD_LIBRARY_PATH#:}

# Export the new PKG_CONFIG_PATH and LD_LIBRARY_PATH
export PKG_CONFIG_PATH
export LD_LIBRARY_PATH

echo "Updated PKG_CONFIG_PATH: $PKG_CONFIG_PATH"
echo "Updated LD_LIBRARY_PATH: $LD_LIBRARY_PATH"
