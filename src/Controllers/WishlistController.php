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

/**
 * Wishlist Controller
 * 
 * Handles all operations related to user wishlists including viewing, creating, 
 * editing, and managing wishlist items with their priorities.
 * 
 * @package SecretSanta\Controllers
 */
class WishlistController extends BaseController
{
    /**
     * Repository instances for data access
     *
     * @var WishlistRepository
     * @var WishlistItemRepository
     * @var GroupRepository
     * @var UserRepository
     * @var GroupMemberRepository
     */
    private WishlistRepository $wishlistRepository;
    private WishlistItemRepository $itemRepository;
    private GroupRepository $groupRepository;
    private UserRepository $userRepository;
    private GroupMemberRepository $memberRepository;

    /**
     * Constructor - initializes repositories for data access
     */
    public function __construct()
    {
        parent::__construct();
        $this->wishlistRepository = new WishlistRepository();
        $this->itemRepository = new WishlistItemRepository();
        $this->groupRepository = new GroupRepository();
        $this->userRepository = new UserRepository();
        $this->memberRepository = new GroupMemberRepository();
    }

    /**
     * Display a wishlist for a user in a specific group
     * 
     * Enforces access control - users can only view their own wishlist or the wishlist
     * of their assigned recipient (if draw has occurred) unless they are the group admin
     * 
     * @param int $userId The ID of the user whose wishlist to view
     * @param int $groupId The ID of the group context
     * @return string|void HTML content or redirect
     */
    public function view(int $userId, int $groupId)
    {
        $this->requireAuth();

        $currentUserId = $this->auth->userId();

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $currentUserId);
        if (!$member) {
            $this->session->setFlash('error', t('flash.error.not_group_member'));
            return $this->redirect('/groups');
        }

        // If viewing someone else's wishlist, verify they are in the same group
        if ($userId !== $currentUserId) {
            $otherMember = $this->memberRepository->findByGroupAndUser($groupId, $userId);
            if (!$otherMember) {
                $this->session->setFlash('error', t('flash.error.user_not_in_group'));
                return $this->redirect('/groups/' . $groupId);
            }

            // For non-admin users, verify they are assigned as this person's Secret Santa
            $group = $this->groupRepository->find($groupId);
            if ($group && $group->isDrawn() && $group->getAdminId() !== $currentUserId) {
                $assignmentRepository = new GiftAssignmentRepository();
                $assignment = $assignmentRepository->findByGiverAndGroup($currentUserId, $groupId);

                if (!$assignment || $assignment->getReceiverId() !== $userId) {
                    $this->session->setFlash('error', t('flash.error.only_view_assigned_recipient'));
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
     * 
     * Creates a new wishlist if it doesn't exist yet for the user and group
     * 
     * @param int $groupId The ID of the group context
     * @return string|void HTML content or redirect
     */
    public function edit(int $groupId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', t('flash.error.not_group_member'));
            return $this->redirect('/groups');
        }

        $group = $this->groupRepository->find($groupId);
        if (!$group) {
            $this->session->setFlash('error', t('flash.error.group_not_found'));
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
     * Updates wishlist properties such as priority ordering setting
     * 
     * @param int $groupId The ID of the group context
     * @return void 
     */
    public function updateSettings(int $groupId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', t('flash.error.not_group_member'));
            return $this->redirect('/groups');
        }

        $isPriorityOrdered = (bool) $this->request->getPostParam('is_priority_ordered', false);

        // Update the wishlist
        $this->wishlistRepository->createOrUpdateWishlist($userId, $groupId, $isPriorityOrdered);

        $this->session->setFlash('success', t('flash.success.wishlist_settings_updated'));
        return $this->redirect('/wishlist/edit/' . $groupId);
    }

    /**
     * Add an item to the wishlist
     * 
     * Creates a new wishlist item with validation of the input data
     * 
     * @param int $groupId The ID of the group context
     * @return void
     */
    public function addItem(int $groupId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', t('flash.error.not_group_member'));
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

        $this->session->setFlash('success', t('flash.success.item_added'));
        return $this->redirect('/wishlist/edit/' . $groupId);
    }

    /**
     * Update an existing wishlist item
     * 
     * Updates an item's details with validation and ownership check
     * 
     * @param int $itemId The ID of the wishlist item to update
     * @return void
     */
    public function updateItem(int $itemId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Get the item
        $item = $this->itemRepository->find($itemId);
        if (!$item) {
            $this->session->setFlash('error', t('flash.error.item_not_found'));
            return $this->redirect('/groups');
        }

        // Load the wishlist to check ownership
        $this->itemRepository->loadWishlist($item);
        $wishlist = $item->getWishlist();

        if (!$wishlist || $wishlist->getUserId() !== $userId) {
            $this->session->setFlash('error', t('flash.error.no_permission_edit_item'));
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

        $this->session->setFlash('success', t('flash.success.item_updated'));
        return $this->redirect('/wishlist/edit/' . $wishlist->getGroupId());
    }

    /**
     * Delete a wishlist item
     * 
     * Removes an item after verifying ownership
     * 
     * @param int $itemId The ID of the wishlist item to delete
     * @return void
     */
    public function deleteItem(int $itemId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Get the item
        $item = $this->itemRepository->find($itemId);
        if (!$item) {
            $this->session->setFlash('error', t('flash.error.item_not_found'));
            return $this->redirect('/groups');
        }

        // Load the wishlist to check ownership
        $this->itemRepository->loadWishlist($item);
        $wishlist = $item->getWishlist();

        if (!$wishlist || $wishlist->getUserId() !== $userId) {
            $this->session->setFlash('error', t('flash.error.no_permission_delete_item'));
            return $this->redirect('/groups');
        }

        // Delete the item
        $this->itemRepository->delete($item);

        $this->session->setFlash('success', t('flash.success.item_deleted'));
        return $this->redirect('/wishlist/edit/' . $wishlist->getGroupId());
    }

    /**
     * Update the priority order of wishlist items
     * 
     * Reorders items based on submitted priority positions
     * 
     * @param int $groupId The ID of the group context
     * @return void
     */
    public function updatePriority(int $groupId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', t('flash.error.not_group_member'));
            return $this->redirect('/groups');
        }

        // Get the wishlist
        $wishlist = $this->wishlistRepository->findByUserAndGroup($userId, $groupId);
        if (!$wishlist) {
            $this->session->setFlash('error', t('flash.error.wishlist_not_found'));
            return $this->redirect('/wishlist/edit/' . $groupId);
        }

        // Get the item positions from the request
        $positions = $this->request->getPostParam('positions', []);
        if (empty($positions) || !is_array($positions)) {
            $this->session->setFlash('error', t('flash.error.invalid_item_positions'));
            return $this->redirect('/wishlist/edit/' . $groupId);
        }

        // Update item positions
        $success = $this->itemRepository->updatePositions($positions);

        if ($success) {
            $this->session->setFlash('success', t('flash.success.priorities_updated'));
        } else {
            $this->session->setFlash('error', t('flash.error.priorities_update_failed'));
        }

        return $this->redirect('/wishlist/edit/' . $groupId);
    }

    /**
     * Delete a user's entire wishlist for a specific group
     * 
     * Removes the wishlist and all its items after verifying ownership
     * 
     * @param int $groupId The ID of the group context
     * @return void
     */
    public function deleteWishlist(int $groupId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', t('flash.error.not_group_member'));
            return $this->redirect('/groups');
        }

        // Get the wishlist
        $wishlist = $this->wishlistRepository->findByUserAndGroup($userId, $groupId);
        if (!$wishlist) {
            $this->session->setFlash('error', t('flash.error.wishlist_not_found'));
            return $this->redirect('/groups/' . $groupId);
        }

        // Delete the wishlist (this will cascade delete all items due to foreign key constraints)
        $success = $this->wishlistRepository->delete($wishlist);

        if ($success) {
            $this->session->setFlash('success', t('flash.success.wishlist_deleted'));
        } else {
            $this->session->setFlash('error', t('flash.error.wishlist_delete_failed'));
        }

        return $this->redirect('/groups/' . $groupId);
    }
}
