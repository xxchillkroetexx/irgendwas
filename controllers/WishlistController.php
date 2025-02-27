<?php
namespace controllers;

use core\Controller;
use models\Group;
use models\User;
use models\Wishlist;

class WishlistController extends Controller {
    // Constructor - require authentication for all methods
    public function __construct() {
        $this->requireAuth();
    }
    
    // Show wishlist
    public function show($groupId, $userId = null) {
        $currentUser = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($groupId);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if current user is member or admin of the group
        if (!$group->isMember($currentUser->getId()) && !$group->isAdmin($currentUser->getId())) {
            $this->flash('danger', 'You are not a member of this group');
            $this->redirect('/groups');
            return;
        }
        
        // Determine which user's wishlist to show
        if ($userId === null) {
            // Show current user's wishlist
            $userId = $currentUser->getId();
            $editable = true;
        } else {
            // Show another user's wishlist
            $userModel = new User();
            $user = $userModel->findById($userId);
            
            if (!$user || !$group->isMember($userId)) {
                $this->flash('danger', 'User not found in this group');
                $this->redirect("/groups/$groupId");
                return;
            }
            
            // Check if user should see this wishlist
            if ($group->getWishlistVisibility() === 'santa_only') {
                $assignment = $group->getUserAssignment($currentUser->getId());
                
                // Can only see wishlist if user is the recipient's Secret Santa or admin
                if (!$group->isAdmin($currentUser->getId()) && 
                    (!$assignment || $assignment->getId() != $userId)) {
                    $this->flash('danger', 'You do not have permission to view this wishlist');
                    $this->redirect("/groups/$groupId");
                    return;
                }
            }
            
            $editable = false;
        }
        
        // Get wishlist
        $wishlistModel = new Wishlist();
        $wishlist = $wishlistModel->findByUserAndGroup($userId, $groupId);
        
        if (!$wishlist) {
            if ($userId == $currentUser->getId()) {
                // Create new wishlist for current user
                $wishlist = $wishlistModel->create($userId, $groupId);
            } else {
                $this->flash('info', 'This user has not created a wishlist yet');
                $this->redirect("/groups/$groupId");
                return;
            }
        }
        
        // Get user
        $userModel = new User();
        $user = $userModel->findById($userId);
        
        $this->view('wishlists/show', [
            'pageTitle' => $user->getFullName() . "'s Wishlist",
            'group' => $group,
            'wishlist' => $wishlist,
            'user' => $user,
            'editable' => $editable,
            'csrf' => $this->generateCSRF()
        ]);
    }
    
    // Add item to wishlist
    public function addItem($groupId) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get wishlist
        $wishlistModel = new Wishlist();
        $wishlist = $wishlistModel->findByUserAndGroup($user->getId(), $groupId);
        
        if (!$wishlist) {
            // Create new wishlist
            $wishlist = $wishlistModel->create($user->getId(), $groupId);
        }
        
        // Check ownership
        if (!$wishlist->belongsToUser($user->getId())) {
            $this->flash('danger', 'You can only edit your own wishlist');
            $this->redirect("/groups/$groupId");
            return;
        }
        
        // Get form data
        $name = $_POST['name'] ?? '';
        $description = $_POST['description'] ?? '';
        $link = $_POST['link'] ?? '';
        
        // Validate input
        if (empty($name)) {
            $this->flash('danger', 'Item name is required');
            $this->redirect("/groups/$groupId/wishlist");
            return;
        }
        
        // Clean link URL
        if (!empty($link) && !filter_var($link, FILTER_VALIDATE_URL)) {
            $link = 'https://' . $link;
            if (!filter_var($link, FILTER_VALIDATE_URL)) {
                $link = null;
            }
        }
        
        // Add item
        $wishlist->addItem($name, $description, $link);
        
        $this->flash('success', 'Item added to your wishlist');
        $this->redirect("/groups/$groupId/wishlist");
    }
    
    // Delete item from wishlist
    public function deleteItem($groupId, $itemId) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get wishlist
        $wishlistModel = new Wishlist();
        $wishlist = $wishlistModel->findByUserAndGroup($user->getId(), $groupId);
        
        if (!$wishlist) {
            $this->flash('danger', 'Wishlist not found');
            $this->redirect("/groups/$groupId");
            return;
        }
        
        // Check ownership
        if (!$wishlist->belongsToUser($user->getId())) {
            $this->flash('danger', 'You can only edit your own wishlist');
            $this->redirect("/groups/$groupId");
            return;
        }
        
        // Delete item
        if ($wishlist->deleteItem($itemId)) {
            $this->flash('success', 'Item removed from your wishlist');
        } else {
            $this->flash('danger', 'Could not remove item from wishlist');
        }
        
        $this->redirect("/groups/$groupId/wishlist");
    }
    
    // Reorder wishlist items
    public function reorderItems($groupId) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get wishlist
        $wishlistModel = new Wishlist();
        $wishlist = $wishlistModel->findByUserAndGroup($user->getId(), $groupId);
        
        if (!$wishlist) {
            $this->json(['success' => false, 'message' => 'Wishlist not found'], 404);
        }
        
        // Check ownership
        if (!$wishlist->belongsToUser($user->getId())) {
            $this->json(['success' => false, 'message' => 'You can only edit your own wishlist'], 403);
        }
        
        // Get item order from POST data
        $data = json_decode(file_get_contents('php://input'), true);
        $itemOrder = $data['items'] ?? [];
        
        if (empty($itemOrder)) {
            $this->json(['success' => false, 'message' => 'No item order provided']);
        }
        
        // Update order
        if ($wishlist->reorderItems($itemOrder)) {
            $this->json(['success' => true]);
        } else {
            $this->json(['success' => false, 'message' => 'Could not update item order']);
        }
    }
    
    // Update wishlist item
    public function updateItem($groupId, $itemId) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get wishlist
        $wishlistModel = new Wishlist();
        $wishlist = $wishlistModel->findByUserAndGroup($user->getId(), $groupId);
        
        if (!$wishlist) {
            $this->flash('danger', 'Wishlist not found');
            $this->redirect("/groups/$groupId");
            return;
        }
        
        // Check ownership
        if (!$wishlist->belongsToUser($user->getId())) {
            $this->flash('danger', 'You can only edit your own wishlist');
            $this->redirect("/groups/$groupId");
            return;
        }
        
        // Get form data
        $name = $_POST['name'] ?? '';
        $description = $_POST['description'] ?? '';
        $link = $_POST['link'] ?? '';
        
        // Validate input
        if (empty($name)) {
            $this->flash('danger', 'Item name is required');
            $this->redirect("/groups/$groupId/wishlist");
            return;
        }
        
        // Clean link URL
        if (!empty($link) && !filter_var($link, FILTER_VALIDATE_URL)) {
            $link = 'https://' . $link;
            if (!filter_var($link, FILTER_VALIDATE_URL)) {
                $link = null;
            }
        }
        
        // Update item
        if ($wishlist->updateItem($itemId, [
            'name' => $name,
            'description' => $description,
            'link' => $link
        ])) {
            $this->flash('success', 'Item updated');
        } else {
            $this->flash('danger', 'Could not update item');
        }
        
        $this->redirect("/groups/$groupId/wishlist");
    }
}