<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title><?= isset($pageTitle) ? $pageTitle . ' - ' . APP_NAME : APP_NAME ?></title>
    
    <!-- Bootstrap CSS -->
    <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.2.3/dist/css/bootstrap.min.css" rel="stylesheet">
    
    <!-- Font Awesome -->
    <link href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.0.0/css/all.min.css" rel="stylesheet">
    
    <!-- Custom styles -->
    <style>
        body {
            padding-top: 4.5rem;
            padding-bottom: 3rem;
            background-color: #f8f9fa;
        }
        .navbar-brand {
            font-weight: bold;
        }
        .jumbotron {
            background-color: #e9ecef;
            padding: 2rem;
            margin-bottom: 2rem;
            border-radius: 0.3rem;
        }
        .wishlist-item {
            background-color: white;
            border-left: 4px solid #dc3545;
            margin-bottom: 1rem;
            padding: 1rem;
            border-radius: 0.25rem;
            box-shadow: 0 0.125rem 0.25rem rgba(0, 0, 0, 0.075);
        }
        .wishlist-item-draggable {
            cursor: move;
        }
        .footer {
            background-color: #f5f5f5;
            padding: 1rem 0;
            margin-top: 2rem;
        }
        .card-santa {
            border-left: 4px solid #28a745;
        }
        @media (max-width: 767.98px) {
            .table-responsive {
                display: block;
                width: 100%;
                overflow-x: auto;
            }
        }
    </style>
</head>
<body>
    <!-- Navigation -->
    <nav class="navbar navbar-expand-md navbar-dark bg-danger fixed-top">
        <div class="container">
            <a class="navbar-brand" href="/">
                <i class="fas fa-gift me-2"></i><?= APP_NAME ?>
            </a>
            <button class="navbar-toggler" type="button" data-bs-toggle="collapse" data-bs-target="#navbarMain">
                <span class="navbar-toggler-icon"></span>
            </button>
            <div class="collapse navbar-collapse" id="navbarMain">
                <ul class="navbar-nav ms-auto">
                    <?php if (isset($_SESSION['user_id'])): ?>
                        <li class="nav-item">
                            <a class="nav-link" href="/groups">My Groups</a>
                        </li>
                        <li class="nav-item dropdown">
                            <a class="nav-link dropdown-toggle" href="#" id="userDropdown" role="button" data-bs-toggle="dropdown">
                                Account
                            </a>
                            <ul class="dropdown-menu dropdown-menu-end">
                                <li><a class="dropdown-item" href="/profile">My Profile</a></li>
                                <li><hr class="dropdown-divider"></li>
                                <li><a class="dropdown-item" href="/logout">Logout</a></li>
                            </ul>
                        </li>
                    <?php else: ?>
                        <li class="nav-item">
                            <a class="nav-link" href="/login">Login</a>
                        </li>
                        <li class="nav-item">
                            <a class="nav-link" href="/register">Register</a>
                        </li>
                    <?php endif; ?>
                </ul>
            </div>
        </div>
    </nav>

    <!-- Flash Messages -->
    <div class="container mt-4">
        <?php
        // Use the Flash utility class instead of controller instance
        $flash = \core\Flash::get();
        foreach ($flash as $type => $message):
            $alertClass = match($type) {
                'success' => 'alert-success',
                'danger' => 'alert-danger',
                'warning' => 'alert-warning',
                'info' => 'alert-info',
                default => 'alert-secondary'
            };
        ?>
        <div class="alert <?= $alertClass ?> alert-dismissible fade show">
            <?= $message ?>
            <button type="button" class="btn-close" data-bs-dismiss="alert"></button>
        </div>
        <?php endforeach; ?>
    </div>

    <!-- Main Content -->
    <main class="container">
        <?= $content ?>
    </main>

    <!-- Footer -->
    <footer class="footer mt-auto py-3">
        <div class="container text-center">
            <span class="text-muted">&copy; <?= date('Y') ?> <?= APP_NAME ?> - Secret Santa Made Easy</span>
        </div>
    </footer>

    <!-- Bootstrap JS and dependencies -->
    <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.2.3/dist/js/bootstrap.bundle.min.js"></script>
    
    <!-- jQuery (for optional features) -->
    <script src="https://code.jquery.com/jquery-3.6.0.min.js"></script>
    
    <!-- SortableJS (for drag-and-drop) -->
    <script src="https://cdn.jsdelivr.net/npm/sortablejs@1.14.0/Sortable.min.js"></script>
    
    <!-- Custom scripts -->
    <script>
        // Initialize all tooltips
        var tooltipTriggerList = [].slice.call(document.querySelectorAll('[data-bs-toggle="tooltip"]'));
        var tooltipList = tooltipTriggerList.map(function (tooltipTriggerEl) {
            return new bootstrap.Tooltip(tooltipTriggerEl);
        });
        
        // Initialize drag-and-drop for wishlist items
        document.addEventListener('DOMContentLoaded', function() {
            const wishlistContainer = document.getElementById('wishlist-items');
            if (wishlistContainer) {
                const sortable = new Sortable(wishlistContainer, {
                    animation: 150,
                    handle: '.wishlist-item-handle',
                    onEnd: function(evt) {
                        const itemOrder = Array.from(wishlistContainer.children).map(item => item.dataset.itemId);
                        
                        // Send reordering to server
                        const groupId = wishlistContainer.dataset.groupId;
                        fetch(`/groups/${groupId}/wishlist/reorder`, {
                            method: 'POST',
                            headers: {
                                'Content-Type': 'application/json',
                                'X-CSRF-Token': document.querySelector('meta[name="csrf-token"]').content
                            },
                            body: JSON.stringify({
                                items: itemOrder
                            })
                        });
                    }
                });
            }
        });
    </script>
</body>
</html>
