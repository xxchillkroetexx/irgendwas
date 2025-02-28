// Main application JavaScript file

document.addEventListener('DOMContentLoaded', function() {
    // Initialize all Bootstrap tooltips
    var tooltipTriggerList = [].slice.call(document.querySelectorAll('[data-bs-toggle="tooltip"]'));
    var tooltipList = tooltipTriggerList.map(function (tooltipTriggerEl) {
        return new bootstrap.Tooltip(tooltipTriggerEl);
    });
    
    // Initialize all Bootstrap popovers
    var popoverTriggerList = [].slice.call(document.querySelectorAll('[data-bs-toggle="popover"]'));
    var popoverList = popoverTriggerList.map(function (popoverTriggerEl) {
        return new bootstrap.Popover(popoverTriggerEl);
    });
    
    // Auto-dismiss alerts after 5 seconds
    setTimeout(function() {
        var alerts = document.querySelectorAll('.alert:not(.alert-permanent)');
        alerts.forEach(function(alert) {
            var bsAlert = new bootstrap.Alert(alert);
            bsAlert.close();
        });
    }, 5000);
    
    // Handle wishlist item sorting
    var wishlistContainer = document.querySelector('.wishlist-items-container');
    if (wishlistContainer) {
        initWishlistSorting();
    }
    
    // Handle priority toggle
    var priorityToggle = document.getElementById('priority-toggle');
    if (priorityToggle) {
        priorityToggle.addEventListener('change', function() {
            var form = this.closest('form');
            if (form) {
                form.submit();
            }
        });
    }
});

function initWishlistSorting() {
    // Get all move up/down buttons
    var moveButtons = document.querySelectorAll('.move-item-btn');
    
    // Add click event listeners
    moveButtons.forEach(function(button) {
        button.addEventListener('click', function(e) {
            e.preventDefault();
            
            var direction = this.dataset.direction;
            var itemId = this.dataset.itemId;
            
            // Submit the move request
            var form = document.createElement('form');
            form.method = 'post';
            form.action = '/wishlist/item/move/' + itemId + '/' + direction;
            document.body.appendChild(form);
            form.submit();
        });
    });
}

// Confirm deletion
function confirmDelete(message, formId) {
    if (confirm(message)) {
        document.getElementById(formId).submit();
        return true;
    }
    return false;
}