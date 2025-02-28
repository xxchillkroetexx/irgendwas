<?php

namespace SecretSanta\Controllers;

use SecretSanta\Models\Wishlist;
use SecretSanta\Models\WishlistItem;
use SecretSanta\Repositories\WishlistRepository;
use SecretSanta\Repositories\WishlistItemRepository;
use SecretSanta\Repositories\GroupRepository;
use SecretSanta\Repositories\UserRepository;
use SecretSanta\Repositories\GroupMemberRepository;
use SecretSanta\Repositories\GiftAssignmentRepository;

class WishlistController extends BaseController {
    private WishlistRepository $wishlistRepository;
    private WishlistItemRepository $itemRepository;
    private GroupRepository $groupRepository;
    private UserRepository $userRepository;
    private GroupMemberRepository $memberRepository;
    
    public function __construct() {
        parent::__construct();
        $this->wishlistRepository = new WishlistRepository();
        $this->itemRepository = new WishlistItemRepository();
        $this->groupRepository = new GroupRepository();
        $this->userRepository = new UserRepository();
        $this->memberRepository = new GroupMemberRepository();
    }
    
    /**
     * Display a wishlist for a user in a specific group
     */
    public function view(int $userId, int $groupId) {
        $this->requireAuth();
        
        $currentUserId = $this->auth->userId();
        
        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $currentUserId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }
        
        // If viewing someone else's wishlist, verify they are in the same group
        if ($userId !== $currentUserId) {
            $otherMember = $this->memberRepository->findByGroupAndUser($groupId, $userId);
            if (!$otherMember) {
                $this->session->setFlash('error', 'This user is not a member of the group');
                return $this->redirect('/groups/' . $groupId);
            }
            
            // For non-admin users, verify they are assigned as this person's Secret Santa
            $group = $this->groupRepository->find($groupId);
            if ($group && $group->isDrawn() && $group->getAdminId() !== $currentUserId) {
                $assignmentRepository = new GiftAssignmentRepository();
                $assignment = $assignmentRepository->findByGiverAndGroup($currentUserId, $groupId);
                
                if (!$assignment || $assignment->getReceiverId() !== $userId) {
                    $this->session->setFlash('error', 'You can only view the wishlist of your assigned recipient');
                    return $this->redirect('/groups/' . $groupId);
                }
            }
        }
        
        // Get the wishlist
        $wishlist = $this->wishlistRepository->findByUserAndGroup($userId, $groupId);
        $user = $this->userRepository->find($userId);
        $group = $this->groupRepository->find($groupId);
        
        if ($wishlist) {
            $this->wishlistRepository->loadItems($wishlist);
        }
        
        return $this->render('wishlist/view', [
            'wishlist' => $wishlist,
            'user' => $user,
            'group' => $group,
            'is_own_wishlist' => ($userId === $currentUserId),
            'page_title' => $user ? $user->getName() . '\'s Wishlist' : 'Wishlist'
        ]);
    }
    
    /**
     * Show the form to edit a wishlist
     */
    public function edit(int $groupId) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        
        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }
        
        $group = $this->groupRepository->find($groupId);
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }
        
        // Get or create the wishlist
        $wishlist = $this->wishlistRepository->findByUserAndGroup($userId, $groupId);
        if (!$wishlist) {
            $wishlist = $this->wishlistRepository->createOrUpdateWishlist($userId, $groupId);
        }
        
        // Load wishlist items
        $this->wishlistRepository->loadItems($wishlist);
        
        return $this->render('wishlist/edit', [
            'wishlist' => $wishlist,
            'group' => $group,
            'page_title' => 'Edit Your Wishlist'
        ]);
    }
    
    /**
     * Update the wishlist settings
     */
    public function updateSettings(int $groupId) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        
        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }
        
        $isPriorityOrdered = (bool) $this->request->getPostParam('is_priority_ordered', false);
        
        // Update the wishlist
        $this->wishlistRepository->createOrUpdateWishlist($userId, $groupId, $isPriorityOrdered);
        
        $this->session->setFlash('success', 'Wishlist settings updated');
        return $this->redirect('/wishlist/edit/' . $groupId);
    }
    
    /**
     * Add an item to the wishlist
     */
    public function addItem(int $groupId) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        
        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }
        
        $title = $this->request->getPostParam('title');
        $description = $this->request->getPostParam('description');
        $link = $this->request->getPostParam('link');
        
        // Validate inputs
        $errors = [];
        
        if (empty($title)) {
            $errors['title'] = 'Item title is required';
        }
        
        if (!empty($link) && !filter_var($link, FILTER_VALIDATE_URL)) {
            $errors['link'] = 'Invalid URL format';
        }
        
        if (!empty($errors)) {
            $this->session->setFlash('errors', $errors);
            $this->session->setFlash('old', $this->request->all());
            return $this->redirect('/wishlist/edit/' . $groupId);
        }
        
        // Get or create wishlist
        $wishlist = $this->wishlistRepository->findByUserAndGroup($userId, $groupId);
        if (!$wishlist) {
            $wishlist = $this->wishlistRepository->createOrUpdateWishlist($userId, $groupId);
        }
        
        // Add the item
        $this->itemRepository->createItem($wishlist, $title, $description, $link);
        
        $this->session->setFlash('success', 'Item added to your wishlist');
        return $this->redirect('/wishlist/edit/' . $groupId);
    }
    
    /**
     * Update an existing wishlist item
     */
    public function updateItem(int $itemId) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        
        // Get the item
        $item = $this->itemRepository->find($itemId);
        if (!$item) {
            $this->session->setFlash('error', 'Item not found');
            return $this->redirect('/groups');
        }
        
        // Load the wishlist to check ownership
        $this->itemRepository->loadWishlist($item);
        $wishlist = $item->getWishlist();
        
        if (!$wishlist || $wishlist->getUserId() !== $userId) {
            $this->session->setFlash('error', 'You do not have permission to edit this item');
            return $this->redirect('/groups');
        }
        
        $title = $this->request->getPostParam('title');
        $description = $this->request->getPostParam('description');
        $link = $this->request->getPostParam('link');
        
        // Validate inputs
        $errors = [];
        
        if (empty($title)) {
            $errors['title'] = 'Item title is required';
        }
        
        if (!empty($link) && !filter_var($link, FILTER_VALIDATE_URL)) {
            $errors['link'] = 'Invalid URL format';
        }
        
        if (!empty($errors)) {
            $this->session->setFlash('errors', $errors);
            $this->session->setFlash('old', $this->request->all());
            return $this->redirect('/wishlist/edit/' . $wishlist->getGroupId());
        }
        
        // Update the item
        $item->setTitle($title)
             ->setDescription($description)
             ->setLink($link);
        
        $this->itemRepository->save($item);
        
        $this->session->setFlash('success', 'Item updated successfully');
        return $this->redirect('/wishlist/edit/' . $wishlist->getGroupId());
    }
    
    /**
     * Delete a wishlist item
     */
    public function deleteItem(int $itemId) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        
        // Get the item
        $item = $this->itemRepository->find($itemId);
        if (!$item) {
            $this->session->setFlash('error', 'Item not found');
            return $this->redirect('/groups');
        }
        
        // Load the wishlist to check ownership
        $this->itemRepository->loadWishlist($item);
        $wishlist = $item->getWishlist();
        
        if (!$wishlist || $wishlist->getUserId() !== $userId) {
            $this->session->setFlash('error', 'You do not have permission to delete this item');
            return $this->redirect('/groups');
        }
        
        // Delete the item
        $this->itemRepository->delete($item);
        
        $this->session->setFlash('success', 'Item deleted successfully');
        return $this->redirect('/wishlist/edit/' . $wishlist->getGroupId());
    }
    
    /**
     * Update the priority order of wishlist items
     */
    public function updatePriority(int $groupId) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        
        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }
        
        // Get the wishlist
        $wishlist = $this->wishlistRepository->findByUserAndGroup($userId, $groupId);
        if (!$wishlist) {
            $this->session->setFlash('error', 'Wishlist not found');
            return $this->redirect('/wishlist/edit/' . $groupId);
        }
        
        // Get the item positions from the request
        $positions = $this->request->getPostParam('positions', []);
        if (empty($positions) || !is_array($positions)) {
            $this->session->setFlash('error', 'Invalid item positions');
            return $this->redirect('/wishlist/edit/' . $groupId);
        }
        
        // Update item positions
        $success = $this->itemRepository->updatePositions($positions);
        
        if ($success) {
            $this->session->setFlash('success', 'Item priorities updated successfully');
        } else {
            $this->session->setFlash('error', 'Failed to update item priorities');
        }
        
        return $this->redirect('/wishlist/edit/' . $groupId);
    }
}