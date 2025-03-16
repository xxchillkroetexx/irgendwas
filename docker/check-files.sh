#!/bin/bash
echo "Checking file structure inside Docker container..."

# Create a folder for the script
mkdir -p /home/kasimir/Documents/Projects/irgendwas.worktrees/new-try-3/docker

# Make script executable
chmod +x /home/kasimir/Documents/Projects/irgendwas.worktrees/new-try-3/docker/check-files.sh

docker exec -it new-try-3-php-1 bash -c '
echo "Directory Structure:"
ls -la /var/www
echo ""
echo "Checking source directory:"
ls -la /var/www/src
echo ""
echo "Checking Core directory:"
ls -la /var/www/src/Core
echo ""
echo "Checking Autoloader file:"
cat /var/www/src/Core/Autoloader.php | head -n 20
echo ""
echo "PHP Version:"
php -v
'