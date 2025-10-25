#!/bin/bash

# Script to copy files from current directory to clipboard, excluding files in .gitignore
# Usage: ./copy-codebase.sh [source_directory]

set -e  # Exit on error

# Source directory (current directory by default)
SOURCE=${1:-.}

# Make path absolute
SOURCE=$(realpath "$SOURCE")

echo "Preparing to copy files from $SOURCE to clipboard (excluding files in .gitignore)..."

# Create a temporary file to store the output
OUTPUT_FILE=$(mktemp)

# Write the header for the directory tree
echo "Directory Tree:" > "$OUTPUT_FILE"

# Check for required commands
if ! command -v tree &> /dev/null; then
    echo "Error: 'tree' command not found. Please install it."
    exit 1
fi

# Detect clipboard command based on environment
if [ -n "$WAYLAND_DISPLAY" ]; then
    # Wayland is running
    if command -v wl-copy &> /dev/null; then
        CLIP_CMD="wl-copy"
    else
        echo "Error: 'wl-copy' not found. Please install wl-clipboard package."
        exit 1
    fi
elif [ -n "$DISPLAY" ]; then
    # X11 is running
    if command -v xclip &> /dev/null; then
        CLIP_CMD="xclip -selection clipboard"
    elif command -v xsel &> /dev/null; then
        CLIP_CMD="xsel --clipboard --input"
    else
        echo "Error: Neither 'xclip' nor 'xsel' found. Please install one of them."
        exit 1
    fi
elif command -v pbcopy &> /dev/null; then
    # macOS
    CLIP_CMD="pbcopy"
else
    echo "Error: Could not find a suitable clipboard utility."
    echo "Please install one of these utilities:"
    echo "  - Wayland: wl-clipboard"
    echo "  - X11: xclip or xsel"
    echo "  - macOS: pbcopy (should be pre-installed)"
    exit 1
fi

# Change to source directory
cd "$SOURCE"

# Create a file list
FILE_LIST=$(mktemp)

# Check if the source is a git repository
if [ -d ".git" ]; then
    # Git repo: use git to list files
    git ls-files > "$FILE_LIST"
    git ls-files --others --exclude-standard >> "$FILE_LIST"
    
    # Generate directory tree
    echo "*$(tree -I ".git" | tail -n +2)*" >> "$OUTPUT_FILE"
else
    # Not a git repo: handle .gitignore manually
    EXCLUDE_PATTERN=".git"
    
    if [ -f ".gitignore" ]; then
        # Build exclusion pattern from .gitignore
        while IFS= read -r line; do
            line=$(echo "$line" | sed 's/#.*//g' | xargs)  # Remove comments and trim
            if [ -n "$line" ]; then
                EXCLUDE_PATTERN="$EXCLUDE_PATTERN|$line"
            fi
        done < .gitignore
    fi
    
    # List files excluding patterns
    find . -type f | grep -v -E "($EXCLUDE_PATTERN)" > "$FILE_LIST"
    
    # Generate directory tree
    echo "*$(tree -I "$EXCLUDE_PATTERN" | tail -n +2)*" >> "$OUTPUT_FILE"
fi

# Process each file
while IFS= read -r file; do
    if [ -f "$file" ]; then
        # Remove leading ./ if present
        file_path="${file#./}"
        
        echo "" >> "$OUTPUT_FILE"
        echo "File \`$file_path\`:" >> "$OUTPUT_FILE"
        echo "\`\`\`" >> "$OUTPUT_FILE"
        cat "$file" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"  # Add a newline before closing backticks
        echo "\`\`\`" >> "$OUTPUT_FILE"
    fi
done < "$FILE_LIST"

# Clean up file list
rm "$FILE_LIST"

# Copy to clipboard
cat "$OUTPUT_FILE" | $CLIP_CMD
echo "Successfully copied to clipboard using $(echo $CLIP_CMD | cut -d ' ' -f1)."

# Clean up output file
rm "$OUTPUT_FILE"

echo "Copy to clipboard completed successfully."