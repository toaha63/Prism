#!/usr/bin/env python3
import sys

def remove_rust_comments_clean(content):
    """
    Removes all Rust comments (//, /* */, ///) and cleans up extra blank lines.
    """
    lines = content.split('\n')
    new_lines = []
    in_block_comment = False
    i = 0
    
    while i < len(lines):
        line = lines[i]
        
        if in_block_comment:
            end_index = line.find('*/')
            if end_index != -1:
                before_comment = line[:line.find('/*')] if '/*' in line else ''
                after_comment = line[end_index + 2:]
                new_line = before_comment + after_comment
                in_block_comment = False
                if new_line.strip():
                    new_lines.append(new_line)
                i += 1
                continue
            else:
                i += 1
                continue
        
        new_line = ''
        j = 0
        in_single_quote = False
        in_double_quote = False
        escape_next = False
        
        while j < len(line):
            ch = line[j]
            
            if escape_next:
                new_line += ch
                escape_next = False
                j += 1
                continue
            
            if ch == '\\':
                new_line += ch
                escape_next = True
                j += 1
                continue
            
            if ch == '"' and not in_single_quote:
                in_double_quote = not in_double_quote
                new_line += ch
                j += 1
                continue
            if ch == '\'' and not in_double_quote:
                in_single_quote = not in_single_quote
                new_line += ch
                j += 1
                continue
            
            if in_single_quote or in_double_quote:
                new_line += ch
                j += 1
                continue
            
            if ch == '/' and j + 1 < len(line) and line[j+1] == '/':
                break
            
            if ch == '/' and j + 1 < len(line) and line[j+1] == '*':
                in_block_comment = True
                j += 2
                while j < len(line):
                    if line[j] == '*' and j + 1 < len(line) and line[j+1] == '/':
                        j += 2
                        in_block_comment = False
                        break
                    j += 1
                if in_block_comment:
                    break
                continue
            
            new_line += ch
            j += 1
        
        if not in_block_comment:
            cleaned_line = new_line.rstrip()
            if cleaned_line:
                new_lines.append(cleaned_line)
            else:
                new_lines.append('')
        
        i += 1
    
    final_lines = []
    previous_empty = False
    for line in new_lines:
        if line == '':
            if not previous_empty:
                final_lines.append('')
                previous_empty = True
        else:
            final_lines.append(line)
            previous_empty = False
    
    return '\n'.join(final_lines)

def process_file(input_file):
    """
    Process a Rust file and overwrite it with cleaned version.
    """
    try:
        with open(input_file, 'r', encoding='utf-8') as f:
            content = f.read()
        
        cleaned_content = remove_rust_comments_clean(content)
        
        with open(input_file, 'w', encoding='utf-8') as f:
            f.write(cleaned_content)
        
        print(f"OK: {input_file} (overwritten)")
        
    except FileNotFoundError:
        print(f"ERROR: File not found: {input_file}")
    except Exception as e:
        print(f"ERROR: {input_file} -> {str(e)}")

def main():
    if len(sys.argv) < 2:
        print("Usage: python clean_rust_comments.py <file1.rs> [file2.rs] [file3.rs] ...")
        sys.exit(1)
    
    input_files = sys.argv[1:]

    
    for input_file in input_files:
        process_file(input_file)
    
    print("Done processing all files.")

if __name__ == "__main__":
    main()
